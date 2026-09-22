# Block processing

## Scope and ownership

This document owns BlockProcessor, OrphanManager, DependencyResolver, block
admission, PP-boundary phase behavior, and committed block delivery. The
[storage contract](storage.md) owns persistent representation and transactions.
The [processing lifecycle](processing-lifecycle.md) owns recovery phase
coordination and the synthetic GetBlocks producer. [NodeService](node-service.md)
owns raw BlockAdded validation and routing.

## PP-boundary phase behavior — settled

For a Rebuild in `PreSeal`:

```text
boundary_seal_blue_score =
    db_pp_blue_score + anticone_finalization_depth
```

This BlueScore approximation is accepted. PreSeal blocks arrive in
consensus-topological order and use storage's `AllowBoundaryIdentities`
policy. Missing parents and merge-set identities may be outside the retained
PP boundary. The materialized retained portion of the PP anticone precedes
PP-future blocks that merge it.

The **first** block whose blue score is at or above the threshold uses
`RequireMaterialized`, not the permissive policy. Only its definite successful
commit changes BlockProcessor's local phase to `PostSeal` and emits the
exact-once `PpBoundarySealed` milestone upward to ResyncEngine. An ambiguous
or failed transaction emits no milestone. `PpBoundarySealed` is never a
command sent back to BlockProcessor.

The [processing lifecycle](processing-lifecycle.md) owns propagation of the
milestone and the prohibition on entering Catchup before ResyncEngine observes
it. `BeginResync` starts BlockProcessor in PostSeal only after successful
reconciliation. All PostSeal materialization is strict.

Before Catchup, a missing dependency under strict policy requires Resync
rather than creating an orphan. During Catchup and Live, unresolved ordinary
dependencies create in-memory orphans. Entering Live preserves valid queued
and orphan work; it does not certify empty worker queues.

The observed retained PP anticone is complete enough for KGI visualization,
without claiming to contain every mathematical anticone block. Boundary
identity and ORIGIN semantics belong to the
[domain model](domain-model.md#pruning-point-boundary-and-origin--settled);
BlockProcessor never creates placeholder block rows or promotes boundary
identities.

## BlockProcessor — settled

One event loop awaits inputs in strict priority:

```text
Command > Intern/resolved block > Notification block > Synthetic block
```

Use explicit higher-priority polling or draining around lower-priority work.
`tokio::select! { biased; ... }` alone does not guarantee this order under
continually ready inputs.

Conceptual commands are:

```text
BeginRebuild
BeginResync
Catchup { lower_bound, ... }
Live
Deactivate
Shutdown
```

A Begin command resets all processor-local run state: phase, source gates,
overlap map and flag, orphan state, and descendants. Begin has no
acknowledgement. `Deactivate` is an acknowledged descendant barrier; Shutdown
is terminal under the shared lifecycle contract.

The local notification gate closes on Begin and opens on Catchup. Continue
polling its receiver while closed and discard received notifications
immediately. Do not accumulate them for later processing. NodeService rejects
an Enabled BlockAdded lacking verbose materialization data before it reaches
this input.

### Admission and materialization

The first block filter checks processor phase and the source gate. Then query
storage for materiality:

- an already materialized block deduplicates and still yields its
  `PersistedBlock` identity to downstream consumers;
- a new block is normalized into `BlockMaterialization` and committed under
  the phase's current storage reference policy; and
- a permanent boundary identity used as the incoming block is an invariant
  violation.

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
The engine-owned `catchup_sent` set and complete-page observation rules belong
to the [processing lifecycle](processing-lifecycle.md).

### Committed block delivery

```rust
struct PersistedBlock {
    point: VspcPoint,
    selected_parent: BlockHash,
}
```

The normal `PersistedBlock` path represents non-Genesis blocks and therefore
has a mandatory selected parent. Genesis is handled by the rebuild/bootstrap
path. No VSPC `added` or `removed` member is Genesis, although a derived VSPC
source may be Genesis.

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
inputs. As a rule of thumb, when occupancy reaches roughly one quarter to one
third of capacity, request frontier hashes to maximize release. Exact capacity
and threshold remain implementation choices.

An isolated orphan below the threshold is acceptable. Silent notification
loss is not a normal assumption: connection loss or a full notification
channel requests recovery, while callbacks intentionally dropped during
subscription activation are covered by the lifecycle's fixed body-tip gate.
No independent age fallback is required.

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
`BlockPersisted`; there is no separate `Satisfied(hash)` queue protocol. It is
a set, not a queue or map.

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
