use std::{collections::VecDeque, future::Future, sync::Arc, time::Instant};

use kgi_model::{
    block::{BlockHash, ConsensusOrder, MaterializedSyncAnchor, ValidatedNodeBlock},
    vspc::VspcChange,
};
use kgi_node::{
    error::NodeError,
    rpc::{CatchupSinkSample, ValidatedRpcClient},
};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use crate::{
    block_processor::{BlockSubmissionError, BlockSyntheticInput},
    vspc_processor::{VspcSubmissionError, VspcSyntheticInput},
};

/// Normalized node operations consumed by the session synchronization pump.
pub(crate) trait SynchronizationRpc: Send + Sync + 'static {
    /// Returns the validated maximum DAA-score distance used by the rolling Catchup trigger.
    fn catchup_max_daa_gap(&self) -> u64;

    /// Obtains the current normalized Catchup sink sample.
    fn catchup_sink_sample(&self) -> impl Future<Output = Result<CatchupSinkSample, NodeError>> + Send;

    /// Obtains one inclusive-anchor-stripped normalized GetBlocks page.
    fn get_blocks(&self, low_hash: BlockHash) -> impl Future<Output = Result<Vec<ValidatedNodeBlock>, NodeError>> + Send;

    /// Obtains one normalized VSPC V2 response from `low_hash`.
    fn virtual_chain_from(&self, low_hash: BlockHash) -> impl Future<Output = Result<VspcChange, NodeError>> + Send;
}

impl SynchronizationRpc for ValidatedRpcClient {
    fn catchup_max_daa_gap(&self) -> u64 {
        self.node_info().consensus.catchup_max_daa_gap()
    }

    async fn catchup_sink_sample(&self) -> Result<CatchupSinkSample, NodeError> {
        self.catchup_sink_sample().await
    }

    async fn get_blocks(&self, low_hash: BlockHash) -> Result<Vec<ValidatedNodeBlock>, NodeError> {
        self.get_blocks(low_hash).await
    }

    async fn virtual_chain_from(&self, low_hash: BlockHash) -> Result<VspcChange, NodeError> {
        self.virtual_chain_from(low_hash).await
    }
}

pub(super) trait BlockInput: Clone + Send + Sync + 'static {
    fn submit(&self, block: ValidatedNodeBlock) -> impl Future<Output = Result<(), BlockSubmissionError>> + Send;
}

impl BlockInput for BlockSyntheticInput {
    async fn submit(&self, block: ValidatedNodeBlock) -> Result<(), BlockSubmissionError> {
        self.submit(block).await
    }
}

pub(super) trait VspcInput: Clone + Send + Sync + 'static {
    fn submit(&self, change: VspcChange) -> impl Future<Output = Result<(), VspcSubmissionError>> + Send;
}

impl VspcInput for VspcSyntheticInput {
    async fn submit(&self, change: VspcChange) -> Result<(), VspcSubmissionError> {
        self.submit(change).await
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MarkerEvidence {
    Unknown,
    Present,
    Removed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RollingMarker {
    sample: CatchupSinkSample,
    /// Evidence that the sampled sink is present on, or was removed from, the observed VSPC progression.
    evidence: MarkerEvidence,
    seen_by_get_blocks: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CatchupReason {
    /// The rolling sink marker was present and remained within the validated DAA gap.
    RollingMarker,
    /// The page-wide maximum consensus order occurred before the final block.
    GlobalMaximumBeforePageEnd,
    /// The normalized GetBlocks page contained fewer than three blocks.
    ShortGetBlocksPage,
    /// The normalized VSPC response contained no added path.
    EmptyVspcPage,
}

/// Catchup eligibility established while retaining the complete triggering page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CatchupTrigger {
    /// Evidence path that established eligibility.
    pub(super) reason: CatchupReason,
    /// Current rolling sink sample when eligibility was established.
    pub(super) sink_sample: CatchupSinkSample,
}

/// Failure produced by the synchronization task before lifecycle translation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SynchronizationError {
    /// The exact node generation returned a typed operation failure.
    Node(NodeError),
    /// The BlockProcessor synthetic endpoint closed while the session was active.
    BlockInputClosed,
    /// The VspcProcessor synthetic endpoint closed while the session was active.
    VspcInputClosed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SynchronizationEvent {
    /// The pump retained a complete page and requires the session's Catchup handoff.
    CatchupReady(CatchupTrigger),
    /// Synchronization ended with a typed active-session failure.
    Failed(SynchronizationError),
}

/// Inert-then-single-start owner of one session synchronization task.
pub(super) struct Synchronization<R: SynchronizationRpc> {
    task: SynchronizationTask<R>,
    cancellation: CancellationToken,
    events: mpsc::UnboundedReceiver<SynchronizationEvent>,
}

enum SynchronizationTask<R: SynchronizationRpc> {
    Inert {
        pump: SynchronizationPump<R, BlockSyntheticInput, VspcSyntheticInput>,
        event_sender: mpsc::UnboundedSender<SynchronizationEvent>,
    },
    Launching,
    Running(JoinHandle<()>),
}

impl<R: SynchronizationRpc> Synchronization<R> {
    /// Constructs task ownership and channels without starting RPC or processor work.
    pub(super) fn new(rpc: Arc<R>, blocks: BlockSyntheticInput, vspc: VspcSyntheticInput) -> Self {
        let (event_sender, events) = mpsc::unbounded_channel();
        let pump = SynchronizationPump::new(rpc, blocks, vspc);
        Self { task: SynchronizationTask::Inert { pump, event_sender }, cancellation: CancellationToken::new(), events }
    }

    /// Starts synchronization from the prepared anchor exactly once.
    pub(super) fn start(&mut self, anchor: MaterializedSyncAnchor) {
        let task = std::mem::replace(&mut self.task, SynchronizationTask::Launching);
        let SynchronizationTask::Inert { mut pump, event_sender } = task else {
            panic!("synchronization starts exactly once");
        };
        let cancellation = self.cancellation.clone();
        let join = tokio::spawn(async move {
            kaspa_core::info!("ProcessingSession synchronization started from anchor {}", anchor.point.hash());
            let result = tokio::select! {
                biased;
                () = cancellation.cancelled() => {
                    kaspa_core::debug!("ProcessingSession synchronization cancelled before Catchup");
                    return;
                },
                result = pump.run_until_catchup(anchor) => result,
            };
            let event = match result {
                Ok(trigger) => {
                    kaspa_core::info!(
                        "ProcessingSession synchronization reached Catchup eligibility: reason={:?}, sink_hash={}, sink_daa_score={}",
                        trigger.reason,
                        trigger.sink_sample.hash,
                        trigger.sink_sample.daa_score,
                    );
                    SynchronizationEvent::CatchupReady(trigger)
                }
                Err(error) => {
                    kaspa_core::warn!("ProcessingSession synchronization failed: {error:?}");
                    SynchronizationEvent::Failed(error)
                }
            };
            event_sender.send(event).expect("ProcessingSession retains the synchronization event receiver until joined shutdown");
            cancellation.cancelled().await;
        });
        self.task = SynchronizationTask::Running(join);
    }

    /// Waits for the next synchronization conclusion requiring session action.
    pub(super) async fn next_event(&mut self) -> Option<SynchronizationEvent> {
        self.events.recv().await
    }

    /// Consumes the synchronization owner and joins any launched work.
    pub(super) async fn shutdown(self) {
        self.cancellation.cancel();
        match self.task {
            SynchronizationTask::Inert { .. } => {}
            SynchronizationTask::Launching => unreachable!("synchronization launch transition does not suspend"),
            SynchronizationTask::Running(join) => {
                let _ = join.await;
            }
        }
        kaspa_core::debug!("ProcessingSession synchronization stopped");
    }
}

impl From<NodeError> for SynchronizationError {
    fn from(error: NodeError) -> Self {
        Self::Node(error)
    }
}

/// Ordered pre-Catchup synchronization state retained by one ProcessingSession.
pub(super) struct SynchronizationPump<R: SynchronizationRpc, B: BlockInput, V: VspcInput> {
    rpc: Arc<R>,
    blocks: B,
    vspc: V,
    marker: Option<RollingMarker>,
    held_page: VecDeque<ValidatedNodeBlock>,
    held_page_evaluated: bool,
    pending_vspc: Option<VspcChange>,
    empty_vspc_page: bool,
}

#[derive(Clone, Copy)]
struct SynchronizationPosition {
    block_cursor: BlockHash,
    vspc_cursor: BlockHash,
}

impl<R: SynchronizationRpc, B: BlockInput, V: VspcInput> SynchronizationPump<R, B, V> {
    pub(super) fn new(rpc: Arc<R>, blocks: B, vspc: V) -> Self {
        Self {
            rpc,
            blocks,
            vspc,
            marker: None,
            held_page: VecDeque::new(),
            held_page_evaluated: false,
            pending_vspc: None,
            empty_vspc_page: false,
        }
    }

    /// Runs the ordered pump from `anchor` until Catchup is eligible, retaining its triggering page.
    pub(super) async fn run_until_catchup(&mut self, anchor: MaterializedSyncAnchor) -> Result<CatchupTrigger, SynchronizationError> {
        let cursor = anchor.point.hash();
        let mut position = SynchronizationPosition { block_cursor: cursor, vspc_cursor: cursor };
        self.initialize_marker(&position).await?;
        loop {
            self.ensure_vspc_response(&position).await?;
            if self.held_page.is_empty() {
                let low_hash = position.block_cursor;
                let page = self.rpc.get_blocks(low_hash).await?;
                kaspa_core::debug!("ProcessingSession fetched GetBlocks page: low_hash={}, blocks={}", low_hash, page.len(),);
                self.held_page = page.into();
                self.held_page_evaluated = false;
            }
            if !self.held_page_evaluated {
                self.held_page_evaluated = true;
                if let Some(trigger) = self.evaluate_complete_page(&position).await? {
                    return Ok(trigger);
                }
            }
            self.dispatch_through_pending_destination(&mut position).await?;
        }
    }

    fn held_page(&self) -> &VecDeque<ValidatedNodeBlock> {
        &self.held_page
    }

    async fn initialize_marker(&mut self, position: &SynchronizationPosition) -> Result<(), SynchronizationError> {
        if self.marker.is_some() {
            return Ok(());
        }
        let sample = self.rpc.catchup_sink_sample().await?;
        let cursor_equal = sample.hash == position.vspc_cursor;
        self.marker = Some(RollingMarker {
            sample,
            evidence: if cursor_equal { MarkerEvidence::Present } else { MarkerEvidence::Unknown },
            seen_by_get_blocks: cursor_equal,
        });
        kaspa_core::debug!(
            "ProcessingSession initialized Catchup marker: hash={}, daa_score={}, cursor_equal={}",
            sample.hash,
            sample.daa_score,
            cursor_equal,
        );
        Ok(())
    }

    async fn ensure_vspc_response(&mut self, position: &SynchronizationPosition) -> Result<(), SynchronizationError> {
        if self.pending_vspc.is_some() || self.empty_vspc_page {
            return Ok(());
        }
        let low_hash = position.vspc_cursor;
        let change = self.rpc.virtual_chain_from(low_hash).await?;
        kaspa_core::debug!(
            "ProcessingSession fetched VSPC page: low_hash={}, removed={}, added={}",
            low_hash,
            change.removed.len(),
            change.added.len(),
        );
        if change.added.is_empty() {
            self.empty_vspc_page = true;
            return Ok(());
        }
        self.observe_marker(&change, position);
        self.pending_vspc = Some(change);
        Ok(())
    }

    async fn evaluate_complete_page(
        &mut self,
        position: &SynchronizationPosition,
    ) -> Result<Option<CatchupTrigger>, SynchronizationError> {
        loop {
            let marker_hash = self.marker.expect("the initial Catchup marker is installed").sample.hash;
            if self.held_page.iter().any(|block| block.hash == marker_hash) {
                self.marker.as_mut().expect("the marker exists").seen_by_get_blocks = true;
            }

            let marker = self.marker.expect("the marker exists");
            if marker.seen_by_get_blocks && marker.evidence != MarkerEvidence::Unknown {
                let refreshed = self.rpc.catchup_sink_sample().await?;
                if marker.evidence == MarkerEvidence::Present
                    && let Some(gap) = refreshed.daa_score.checked_sub(marker.sample.daa_score)
                    && gap <= self.rpc.catchup_max_daa_gap()
                {
                    return Ok(Some(CatchupTrigger { reason: CatchupReason::RollingMarker, sink_sample: marker.sample }));
                }
                kaspa_core::debug!(
                    "ProcessingSession replaced rolling Catchup marker: previous_hash={}, previous_daa_score={}, replacement_hash={}, replacement_daa_score={}, evidence={:?}",
                    marker.sample.hash,
                    marker.sample.daa_score,
                    refreshed.hash,
                    refreshed.daa_score,
                    marker.evidence,
                );
                self.replace_marker(refreshed, position);
                let replacement = self.marker.expect("the replacement marker exists");
                if replacement.seen_by_get_blocks && replacement.evidence != MarkerEvidence::Unknown {
                    continue;
                }
            }
            break;
        }

        let reason = if self.empty_vspc_page {
            Some(CatchupReason::EmptyVspcPage)
        } else if self.held_page.len() < 3 {
            Some(CatchupReason::ShortGetBlocksPage)
        } else if page_global_maximum_precedes_end(&self.held_page) {
            Some(CatchupReason::GlobalMaximumBeforePageEnd)
        } else {
            None
        };
        Ok(reason.map(|reason| CatchupTrigger { reason, sink_sample: self.marker.expect("the marker exists").sample }))
    }

    fn replace_marker(&mut self, sample: CatchupSinkSample, position: &SynchronizationPosition) {
        let pending_contains = self.pending_vspc.as_ref().is_some_and(|change| change.added.contains(&sample.hash));
        let block_cursor = position.block_cursor;
        let vspc_cursor = position.vspc_cursor;
        self.marker = Some(RollingMarker {
            sample,
            evidence: if sample.hash == vspc_cursor || pending_contains { MarkerEvidence::Present } else { MarkerEvidence::Unknown },
            seen_by_get_blocks: sample.hash == block_cursor || self.held_page.iter().any(|block| block.hash == sample.hash),
        });
    }

    fn observe_marker(&mut self, change: &VspcChange, position: &SynchronizationPosition) {
        let vspc_cursor = position.vspc_cursor;
        let marker = self.marker.as_mut().expect("the initial Catchup marker is installed");
        marker.evidence = match marker.evidence {
            MarkerEvidence::Present if change.removed.contains(&marker.sample.hash) => MarkerEvidence::Removed,
            MarkerEvidence::Unknown if vspc_cursor == marker.sample.hash || change.added.contains(&marker.sample.hash) => {
                MarkerEvidence::Present
            }
            evidence => evidence,
        };
    }

    async fn dispatch_through_pending_destination(
        &mut self,
        position: &mut SynchronizationPosition,
    ) -> Result<(), SynchronizationError> {
        let started = Instant::now();
        let Some(change) = self.pending_vspc.as_ref() else {
            assert!(self.empty_vspc_page, "a nonempty VSPC response supplies a pending destination");
            let accepted = self.held_page.len();
            while let Some(block) = self.held_page.pop_front() {
                position.block_cursor = block.hash;
                self.blocks.submit(block).await.map_err(|_| SynchronizationError::BlockInputClosed)?;
            }
            kaspa_core::debug!(
                "ProcessingSession dispatched synthetic page: blocks={}, vspc_changes=0, remaining_blocks=0, elapsed_ms={}",
                accepted,
                started.elapsed().as_millis(),
            );
            return Ok(());
        };
        let destination = *change.added.last().expect("a pending VSPC response has a destination");
        let destination_position = self.held_page.iter().position(|block| block.hash == destination);
        let count = destination_position.map_or(self.held_page.len(), |position| position + 1);
        for _ in 0..count {
            let block = self.held_page.pop_front().expect("the selected dispatch prefix exists");
            position.block_cursor = block.hash;
            self.blocks.submit(block).await.map_err(|_| SynchronizationError::BlockInputClosed)?;
        }
        if destination_position.is_none() {
            kaspa_core::debug!(
                "ProcessingSession dispatched synthetic page prefix: blocks={}, vspc_changes=0, remaining_blocks={}, destination_pending=true, elapsed_ms={}",
                count,
                self.held_page.len(),
                started.elapsed().as_millis(),
            );
            return Ok(());
        }

        let change = self.pending_vspc.take().expect("the destination belongs to the pending VSPC response");
        let removed = change.removed.len();
        let added = change.added.len();
        self.vspc.submit(change).await.map_err(|_| SynchronizationError::VspcInputClosed)?;
        position.vspc_cursor = destination;
        kaspa_core::debug!(
            "ProcessingSession dispatched synthetic page prefix: blocks={}, vspc_changes=1, vspc_removed={}, vspc_added={}, remaining_blocks={}, destination_pending=false, elapsed_ms={}",
            count,
            removed,
            added,
            self.held_page.len(),
            started.elapsed().as_millis(),
        );
        Ok(())
    }
}

fn page_global_maximum_precedes_end(page: &VecDeque<ValidatedNodeBlock>) -> bool {
    let Some(final_block) = page.back() else {
        return false;
    };
    let final_order = ConsensusOrder::new(final_block.blue_work, final_block.hash);
    page.iter().any(|block| ConsensusOrder::new(block.blue_work, block.hash) > final_order)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use kgi_model::block::{BlueWork, CompactId, VspcPoint};

    use super::*;

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Call {
        Sample,
        Blocks(BlockHash),
        Vspc(BlockHash),
        Block(BlockHash),
        Change(BlockHash),
    }

    struct MockRpc {
        max_gap: u64,
        samples: Mutex<VecDeque<Result<CatchupSinkSample, NodeError>>>,
        pages: Mutex<VecDeque<Result<Vec<ValidatedNodeBlock>, NodeError>>>,
        changes: Mutex<VecDeque<Result<VspcChange, NodeError>>>,
        calls: Arc<Mutex<Vec<Call>>>,
    }

    impl SynchronizationRpc for MockRpc {
        fn catchup_max_daa_gap(&self) -> u64 {
            self.max_gap
        }

        async fn catchup_sink_sample(&self) -> Result<CatchupSinkSample, NodeError> {
            self.calls.lock().expect("call log").push(Call::Sample);
            self.samples.lock().expect("samples").pop_front().expect("scripted sample")
        }

        async fn get_blocks(&self, low_hash: BlockHash) -> Result<Vec<ValidatedNodeBlock>, NodeError> {
            self.calls.lock().expect("call log").push(Call::Blocks(low_hash));
            self.pages.lock().expect("pages").pop_front().expect("scripted page")
        }

        async fn virtual_chain_from(&self, low_hash: BlockHash) -> Result<VspcChange, NodeError> {
            self.calls.lock().expect("call log").push(Call::Vspc(low_hash));
            self.changes.lock().expect("changes").pop_front().expect("scripted VSPC response")
        }
    }

    #[derive(Clone)]
    struct RecordingBlocks(Arc<Mutex<Vec<Call>>>);

    impl BlockInput for RecordingBlocks {
        async fn submit(&self, block: ValidatedNodeBlock) -> Result<(), BlockSubmissionError> {
            self.0.lock().expect("call log").push(Call::Block(block.hash));
            Ok(())
        }
    }

    #[derive(Clone)]
    struct RecordingVspc(Arc<Mutex<Vec<Call>>>);

    type TestPump = SynchronizationPump<MockRpc, RecordingBlocks, RecordingVspc>;

    impl VspcInput for RecordingVspc {
        async fn submit(&self, change: VspcChange) -> Result<(), VspcSubmissionError> {
            self.0.lock().expect("call log").push(Call::Change(*change.added.last().expect("destination")));
            Ok(())
        }
    }

    fn hash(byte: u8) -> BlockHash {
        BlockHash::from_bytes([byte; 32])
    }

    fn block(byte: u8, work: u64) -> ValidatedNodeBlock {
        ValidatedNodeBlock {
            hash: hash(byte),
            selected_parent: hash(byte.saturating_sub(1)),
            direct_parents: vec![hash(byte.saturating_sub(1))],
            blue_merge_set: Vec::new(),
            red_merge_set: Vec::new(),
            timestamp: u64::from(byte),
            daa_score: u64::from(byte),
            blue_score: u64::from(byte),
            blue_work: BlueWork::from_u64(work),
        }
    }

    fn change(added: &[u8], removed: &[u8]) -> VspcChange {
        VspcChange {
            removed: removed.iter().copied().map(hash).collect::<Vec<_>>().into(),
            added: added.iter().copied().map(hash).collect::<Vec<_>>().into(),
        }
    }

    fn anchor() -> MaterializedSyncAnchor {
        MaterializedSyncAnchor {
            point: VspcPoint::new(ConsensusOrder::new(BlueWork::from_u64(1), hash(1)), CompactId::new(1).expect("ID")),
            selected_parent: hash(0),
            blue_score: 1,
        }
    }

    fn pump(
        max_gap: u64,
        samples: Vec<Result<CatchupSinkSample, NodeError>>,
        pages: Vec<Result<Vec<ValidatedNodeBlock>, NodeError>>,
        changes: Vec<Result<VspcChange, NodeError>>,
    ) -> (TestPump, Arc<Mutex<Vec<Call>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let rpc = Arc::new(MockRpc {
            max_gap,
            samples: Mutex::new(samples.into()),
            pages: Mutex::new(pages.into()),
            changes: Mutex::new(changes.into()),
            calls: calls.clone(),
        });
        (SynchronizationPump::new(rpc, RecordingBlocks(calls.clone()), RecordingVspc(calls.clone())), calls)
    }

    #[tokio::test]
    async fn preserves_destination_order_and_holds_the_empty_vspc_fallback_page() {
        let (mut pump, calls) = pump(
            10,
            vec![Ok(CatchupSinkSample { hash: hash(9), daa_score: 9 })],
            vec![Ok(vec![block(2, 2), block(3, 3), block(4, 4)]), Ok(vec![block(5, 5), block(6, 6), block(7, 7)])],
            vec![Ok(change(&[3], &[])), Ok(change(&[], &[]))],
        );

        let trigger = pump.run_until_catchup(anchor()).await.expect("Catchup fallback");
        assert_eq!(trigger.reason, CatchupReason::EmptyVspcPage);
        assert_eq!(pump.held_page().iter().map(|block| block.hash).collect::<Vec<_>>(), vec![hash(5), hash(6), hash(7)]);
        assert_eq!(
            *calls.lock().expect("call log"),
            vec![
                Call::Sample,
                Call::Vspc(hash(1)),
                Call::Blocks(hash(1)),
                Call::Block(hash(2)),
                Call::Block(hash(3)),
                Call::Change(hash(3)),
                Call::Vspc(hash(3)),
                Call::Block(hash(4)),
                Call::Blocks(hash(4)),
            ]
        );
    }

    #[tokio::test]
    async fn rolling_marker_triggers_before_dispatching_its_complete_page() {
        let marker = CatchupSinkSample { hash: hash(3), daa_score: 30 };
        let (mut pump, calls) = pump(
            10,
            vec![Ok(marker), Ok(CatchupSinkSample { hash: hash(3), daa_score: 35 })],
            vec![Ok(vec![block(2, 2), block(3, 3), block(4, 4)])],
            vec![Ok(change(&[3], &[]))],
        );

        let trigger = pump.run_until_catchup(anchor()).await.expect("rolling marker");
        assert_eq!(trigger, CatchupTrigger { reason: CatchupReason::RollingMarker, sink_sample: marker });
        assert_eq!(pump.held_page().len(), 3);
        assert!(!calls.lock().expect("call log").iter().any(|call| matches!(call, Call::Block(_) | Call::Change(_))));
    }

    #[tokio::test]
    async fn cursor_equal_marker_is_present_without_reappearing_in_vspc_or_get_blocks() {
        let marker = CatchupSinkSample { hash: hash(1), daa_score: 10 };
        let (mut pump, calls) = pump(
            5,
            vec![Ok(marker), Ok(CatchupSinkSample { hash: hash(1), daa_score: 15 })],
            vec![Ok(vec![block(2, 2), block(3, 3), block(4, 4)])],
            vec![Ok(change(&[4], &[]))],
        );

        let trigger = pump.run_until_catchup(anchor()).await.expect("cursor-equal marker");
        assert_eq!(trigger, CatchupTrigger { reason: CatchupReason::RollingMarker, sink_sample: marker });
        assert_eq!(pump.held_page().len(), 3);
        assert_eq!(calls.lock().expect("call log").iter().filter(|call| **call == Call::Sample).count(), 2);
    }

    #[tokio::test]
    async fn removed_marker_is_replaced_before_it_can_authorize_catchup() {
        let replacement = CatchupSinkSample { hash: hash(4), daa_score: 40 };
        let (mut pump, _calls) = pump(
            10,
            vec![Ok(CatchupSinkSample { hash: hash(1), daa_score: 10 }), Ok(replacement), Ok(replacement)],
            vec![Ok(vec![block(2, 2), block(3, 3), block(4, 4)])],
            vec![Ok(change(&[4], &[1]))],
        );

        let trigger = pump.run_until_catchup(anchor()).await.expect("replacement marker");
        assert_eq!(trigger, CatchupTrigger { reason: CatchupReason::RollingMarker, sink_sample: replacement });
        assert_eq!(pump.held_page().len(), 3);
    }

    #[tokio::test]
    async fn failed_refresh_retains_the_complete_page_without_dispatch_or_replacement() {
        let marker = CatchupSinkSample { hash: hash(3), daa_score: 30 };
        let error = NodeError::RpcRequestFailed { diagnostic: Arc::from("refresh unavailable") };
        let (mut pump, calls) = pump(
            10,
            vec![Ok(marker), Err(error.clone())],
            vec![Ok(vec![block(2, 2), block(3, 3), block(4, 4)])],
            vec![Ok(change(&[3], &[]))],
        );

        assert_eq!(pump.run_until_catchup(anchor()).await, Err(SynchronizationError::Node(error)));
        assert_eq!(pump.marker.expect("marker retained").sample, marker);
        assert_eq!(pump.held_page().len(), 3);
        assert!(!calls.lock().expect("call log").iter().any(|call| matches!(call, Call::Block(_) | Call::Change(_))));
    }

    #[tokio::test]
    async fn replacement_already_in_the_held_page_is_reconsidered_immediately() {
        let replacement = CatchupSinkSample { hash: hash(4), daa_score: 100 };
        let (mut pump, _calls) = pump(
            10,
            vec![Ok(CatchupSinkSample { hash: hash(3), daa_score: 30 }), Ok(replacement), Ok(replacement)],
            vec![Ok(vec![block(2, 2), block(3, 3), block(4, 4)])],
            vec![Ok(change(&[3, 4], &[]))],
        );

        let trigger = pump.run_until_catchup(anchor()).await.expect("replacement marker");
        assert_eq!(trigger, CatchupTrigger { reason: CatchupReason::RollingMarker, sink_sample: replacement });
        assert_eq!(pump.held_page().len(), 3);
    }

    #[tokio::test]
    async fn page_fallbacks_are_independent_and_preserve_the_page() {
        let fixtures = [
            (vec![block(2, 2), block(3, 9), block(4, 4)], CatchupReason::GlobalMaximumBeforePageEnd),
            (vec![block(2, 2), block(3, 3)], CatchupReason::ShortGetBlocksPage),
        ];
        for (page, reason) in fixtures {
            let expected_len = page.len();
            let (mut pump, _calls) =
                pump(10, vec![Ok(CatchupSinkSample { hash: hash(9), daa_score: 90 })], vec![Ok(page)], vec![Ok(change(&[8], &[]))]);
            let trigger = pump.run_until_catchup(anchor()).await.expect("page fallback");
            assert_eq!(trigger.reason, reason);
            assert_eq!(pump.held_page().len(), expected_len);
        }
    }

    #[tokio::test]
    async fn failed_initial_sample_starts_no_scan_or_dispatch() {
        let error = NodeError::RpcRequestFailed { diagnostic: Arc::from("sample unavailable") };
        let (mut pump, calls) = pump(10, vec![Err(error.clone())], Vec::new(), Vec::new());

        assert_eq!(pump.run_until_catchup(anchor()).await, Err(SynchronizationError::Node(error)));
        assert_eq!(*calls.lock().expect("call log"), vec![Call::Sample]);
    }
}
