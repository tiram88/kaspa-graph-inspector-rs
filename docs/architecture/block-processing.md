# Block processing

## Scope and ownership

This document owns BlockProcessor, OrphanManager, DependencyResolver, block
admission, PP-boundary phase behavior, and committed block delivery. The
[storage contract](storage.md) owns persistent representation and transactions.
The [processing lifecycle](processing-lifecycle.md) owns recovery phase
coordination and the synthetic GetBlocks producer. [NodeService](node-service.md)
owns raw BlockAdded validation and routing.

## PP-boundary phase behavior — settled

For a Rebuild, the effective threshold is:

```text
boundary_seal_blue_score =
    db_pp_blue_score
        when db_pp == network Genesis

    db_pp_blue_score + anticone_finalization_depth
        otherwise
```

Genesis has no discarded DAG past, so a Genesis PP is intrinsically sealed at
blue score zero. After the atomic Genesis rebuild transaction, `BeginRebuild`
starts BlockProcessor directly in `PostSeal` and emits the existing exact-once
`PpBoundarySealed` milestone while handling Begin. Genesis never enters the
ordinary `BlockMaterialization` or `PersistedBlock` path.

For every non-Genesis PP, the BlueScore approximation above is accepted and
`BeginRebuild` starts in `PreSeal`. PreSeal blocks arrive in
consensus-topological order and use storage's `AllowBoundaryIdentities`
policy. Missing parents and merge-set identities may be outside the retained
PP boundary. The materialized retained portion of the PP anticone precedes
PP-future blocks that merge it.

For a non-Genesis PP, the **first** block whose blue score is at or above the
threshold uses `RequireMaterialized`, not the permissive policy. Only its
definite successful commit changes BlockProcessor's local phase to `PostSeal` and emits the
exact-once `PpBoundarySealed` milestone upward to ResyncEngine. An ambiguous
or failed transaction emits no milestone. `PpBoundarySealed` is never a
command sent back to BlockProcessor.

The [processing lifecycle](processing-lifecycle.md) owns propagation of the
milestone and the prohibition on entering Catchup before ResyncEngine observes
it. `BeginResync` starts BlockProcessor in PostSeal only after successful
reconciliation. All PostSeal materialization is strict.

If the process stops after the Genesis rebuild transaction commits but before
the milestone is observed, restart derives the sealed boundary from committed
storage and ordinary Resync starts PostSeal. No replayed milestone or special
repair transaction is required.

Before Catchup, a missing dependency under strict policy requires Resync
rather than creating an orphan. During Catchup and Live, unresolved ordinary
dependencies create in-memory orphans. Entering Live preserves valid queued
and orphan work; it does not certify empty worker queues.

The observed retained PP anticone is complete enough for KGI visualization,
without claiming to contain every mathematical anticone block. Boundary
identity and ORIGIN semantics belong to the
[domain model](domain-model.md#pruning-point-boundary-and-origin--settled), and
their identity-only persistence belongs to
[storage](storage.md#persistent-representation--settled).

## BlockProcessor — settled

One event loop awaits inputs in strict priority:

```text
Command > Intern/resolved block > Notification block > Synthetic block
```

Use explicit higher-priority polling or draining around lower-priority work.
`tokio::select! { biased; ... }` alone does not guarantee this order under
continually ready inputs.

The Begin variants use the
[`BlockProcessorBegin`](processing-lifecycle.md#processor-begin-payloads)
payload owned by the processing lifecycle. Conceptual commands are:

```text
BeginRebuild(BlockProcessorBegin)
BeginResync(BlockProcessorBegin)
Catchup { lower_bound, ... }
Live
Deactivate
Shutdown
```

A Begin command resets all processor-local run state: phase, source gates,
overlap map and flag, orphan state, and descendants. It installs the exact DB
and RPC generations for the run and activates DependencyResolver with that RPC
generation. `BeginResync` requires the anchor blue score to satisfy the supplied
seal threshold and starts in PostSeal. `BeginRebuild` compares the anchor hash
with the validated node's Genesis hash: Genesis starts in PostSeal and emits
`PpBoundarySealed` while handling Begin; every other pruning point starts in
`PreSeal { seal_blue_score: boundary_seal_blue_score }`.

Begin has no acknowledgement. `Deactivate` is an acknowledged descendant
barrier; Shutdown is terminal under the shared lifecycle contract. No separate
Genesis flag, recovery-mode field, or boundary-phase field is carried in the
payload.

The local notification gate closes on Begin and opens on Catchup. Continue
polling its receiver while closed and discard received notifications
immediately. Do not accumulate them for later processing. NodeService rejects
an Enabled BlockAdded lacking verbose materialization data before it reaches
this input.

### Admission and materialization

The first block filter checks processor phase and the source gate. It then
calls `ValidatedDbClient::block_presence(block_hash)`:

- an already materialized block deduplicates and still yields its
  `PersistedBlock` identity to downstream consumers;
- a new block is normalized into `BlockMaterialization` and committed under
  the phase's current storage reference policy; and
- a permanent boundary identity used as the incoming block is an invariant
  violation.

Reference validation and complete missing-reference reporting occur inside the
atomic `ValidatedDbClient::materialize_block(block, policy)` transaction. This
avoids splitting readiness from the commit that relies on it.

PreSeal boundary absences are accepted only through
`AllowBoundaryIdentities`. Strict pre-Catchup missing material requires
Resync. Catchup and Live admit blocks with unresolved ordinary dependencies
to OrphanManager instead of persisting partial state. Transaction validation,
hash interning, coordinate allocation, and commit behavior belong to the
[block materialization transaction](storage.md#block-materialization-transaction--settled).

### Catchup filtering and overlap

Catchup supplies an objective compact-ID lower bound from the latest accepted
GetBlocks anchor or page. Conceptually:

```rust
struct BlockCatchup {
    known_id_lower_bound: CompactId,
}
```

A known BlockAdded below this lower bound is a latecomer and is discarded
without overlap credit. Do not reject an unknown hash using guessed consensus
order. A notification proven below the sealed PP is discarded as invalid.
Legitimate delayed notifications at or above the bound may deduplicate,
materialize, or orphan normally.

Begin resets BlockProcessor's overlap state. Catchup starts accounting with:

```text
SourceBits::SYNTHETIC
SourceBits::NOTIFICATION
HashMap<BlockHash, SourceBits>
AtomicBool block_overlap
```

A hash admitted from both sources proves block overlap and sets the flag.
A second BlockAdded for the same hash within one subscription is an invariant
fault. GetBlocks hashes may repeat across responses, so ResyncEngine filters
synthetic repeats before dispatch; a filtered repeat earns no overlap credit.
Complete-page observation and synthetic repeat-filtering rules belong to the
[processing lifecycle](processing-lifecycle.md).

### Committed block delivery

```rust
struct PersistedBlock {
    point: VspcPoint,
    selected_parent: BlockHash,
}
```

Every `PersistedBlock` delivered by BlockProcessor represents a non-Genesis
block and therefore has a mandatory selected parent. Genesis never enters the
ordinary `BlockMaterialization` or BlockProcessor-to-consumer `PersistedBlock`
path; VspcProcessor instead constructs its initial history record from the
Begin anchor. No VSPC `added` or `removed` member is Genesis, although a derived
VSPC source may be Genesis.

After a definite successful insert, BlockProcessor sends the block's
`BlockCommitted` graph update before delivering `PersistedBlock` to
VspcProcessor and OrphanManager. This preserves graph observer causal order.
A dedup produces no new graph mutation but still delivers `PersistedBlock`.
The [API contract](api.md#in-process-api-and-graph-observer-feed--settled)
owns observer invalidation when graph delivery fails.

`PersistedBlock` delivery is asynchronous but cannot be silently lost after a
successful commit. A full bounded destination loses session continuity and
requires Resync; a closed or unavailable receiver is a session ownership
fault. Cancellation during expected teardown is not a fault. A later session
rederives authoritative state from the database.

## OrphanManager — settled

OrphanManager is an asynchronous child owned by BlockProcessor. It has a
prioritized unbounded command input and a bounded data-message input so
lifecycle commands remain prompt.

It owns:

- in-memory orphan blocks;
- the missing-dependency topology and frontier;
- reverse missing-hash-to-waiting-orphan indexes;
- `resolution_pending: HashSet<BlockHash>`;
- dependency request selection; and
- cancellation when natural arrival makes a request unnecessary.

Messages include new orphan, `BlockPersisted`, dependency result/failure, and
capacity/topology updates. `BlockPersisted` resolves every affected edge.
Every newly ready orphan is removed from orphan storage and returned through
BlockProcessor's Intern/resolved lane. No waiter belongs in the committed
block index, and deduplicated hashes are harmless.

Begin and Deactivate clear or cancel run-local state. Deactivate acknowledges
only after descendants have stopped. After a resolver RPC succeeds, keep its
hash in `resolution_pending` until OrphanManager observes `AddOrphan` or
`BlockPersisted` for that hash. RPC completion alone must not reopen the
request gap before BlockProcessor accounts for the result.

Dependency selection uses orphan topology only. Age and DAA score are not
inputs. When occupancy reaches a configured threshold in the approximate range
from one quarter through one third of capacity, request frontier hashes to
maximize release. Exact processor-channel and orphan capacities, resolver
concurrency, and the orphan threshold remain deferred in the
[decision register](../decisions/deferred.md).

An isolated orphan below the threshold is acceptable. Connection loss or a
full notification channel requests recovery. Callbacks intentionally dropped
during subscription activation are not replayed. If an omitted block later
becomes a dependency of an admitted block, normal dependency resolution
exposes it. No independent age fallback is required.

## DependencyResolver — settled

DependencyResolver is owned for lifecycle by BlockProcessor and driven by
OrphanManager. It has:

- separate command and bounded work channels;
- the processing session's exact `Arc<ValidatedRpcClient>`;
- bounded concurrent GetBlock tasks;
- cancellation for outstanding tasks; and
- no database transaction held during node RPC.

OrphanManager sends Resolve work and `Cancel(hash)` when a block arrives
naturally. It owns the pending set and prevents duplicate requests. Resolver
results return on BlockProcessor's Intern/resolved lane, never its
notification lane. The resolver validates that GetBlock returned the
requested hash.

`resolution_pending` remains set until the manager observes `AddOrphan` or
`BlockPersisted`. It is a set, not a queue or map.

A resolver-confirmed unavailable dependency requests `Require(Rebuild)`
because retained database contents can no longer be trusted against node
state. RPC or connection failure does not prove unavailability and follows
the validated-client/session fault path.

For OrphanManager-to-Resolver bounded work sends, full requires Resync,
closed or unavailable is a session fault, and cancellation during teardown is
expected. Deactivation cancels and joins tasks and releases descendant
session-resource clones before acknowledging. BlockProcessor continues
draining and discarding child results while joining descendants so a full
result channel cannot deadlock the barrier.
