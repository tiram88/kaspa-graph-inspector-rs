# VSPC processing

## Scope and ownership

This document owns VspcProcessor sequencing, readiness, pending indexes,
history, Catchup overlap, notification crossing, and committed VSPC graph
publication. [NodeService](node-service.md) owns raw notification filtering.
The [processing lifecycle](processing-lifecycle.md) owns synthetic production
and phase coordination. [Storage](storage.md) owns the atomic VSPC mutation.

## Change semantics — settled

Rusty-kaspa orders a virtual selected-parent-chain change as:

```text
removed: old sink backward toward common ancestor, ancestor excluded
added:   common-ancestor child forward to new sink
```

Source and destination hashes derive as:

```text
source = removed.first()                 if removed is nonempty
       = selected_parent(added.first())  otherwise

destination = added.last()
```

The shared `VspcChange` and `ReadyVspcChange` representations are defined in
the [domain model](domain-model.md#vspc-value-types--settled). Pending changes
store `VspcChange` directly; there is no `PendingVspcChange` wrapper or
`ReadyAddedBlock`.

Every admitted nonempty change has a nonempty `added` vector. A removed-only
change is impossible under the selected-sink monotonicity assumption reviewed
by the [PUAR](verification.md#pinned-upstream-assumption-review-policy) and has
no invented fallback destination. NodeService rejects that notification shape
with `Require(Resync)`. The synthetic pump rejects it under the bounded typed
Retry policy. Neither source admits it to VspcProcessor.

NodeService also discards a raw notification with both vectors empty before
constructing or sending `VspcChange`. It earns no overlap credit and consumes
no processor capacity. An empty VSPC V2 page is synchronization information
owned by the pump, not a VSPC event. `added` and `removed` never contain
Genesis; a derived source can be Genesis for the first transition.

## Processor lifecycle and gates — settled

The Begin variants use the
[`VspcProcessorBegin`](processing-lifecycle.md#processor-begin-payloads)
payload owned by the processing lifecycle. Conceptual commands are
`BeginRebuild(VspcProcessorBegin)`, `BeginResync(VspcProcessorBegin)`,
`Catchup`, `Live`, `Deactivate`, and `Shutdown`. Commands have priority over
data inputs. VspcProcessor receives no RPC client.

A Begin command resets all run-local pending state, history, overlap, and
phase state, installs the supplied DB generation, and closes the local
notification gate. It sets `committed_vspc_sink = anchor.point` and seeds the
new history from the same anchor as defined below, then enters the pre-Catchup
synthetic-priority phase. Continue polling the
notification receiver while the gate is closed and discard notifications
immediately rather than accumulating them. Catchup opens the gate with its
objective synthetic-sink lower bound. Begin has no acknowledgement. Deactivate
clears run-local state, releases the processing session's DB client clone, and
acknowledges only after the local barrier is complete.

## Pending state and history — settled

Resolution supports both event-before-block and block-before-event. Keep:

- one ordered synthetic FIFO;
- raw unresolved notifications indexed by destination hash;
- resolved notification candidates ordered by destination `ConsensusOrder`;
- pending ID to pending change;
- missing hash to waiting pending IDs;
- hash to recent `PersistedBlock` history; and
- `ConsensusOrder` to hash for ordered pruning and history lookup.

The dual-indexed materialization history is conceptually:

```rust
struct MaterializedHistory {
    by_hash: HashMap<BlockHash, PersistedBlock>,
    by_order: BTreeMap<ConsensusOrder, BlockHash>,
}
```

After clearing the prior run's history, either Begin command constructs and
inserts this seed into both indexes:

```rust
PersistedBlock {
    point: anchor.point,
    selected_parent: anchor.selected_parent,
}
```

For a Genesis anchor the selected parent is synthetic ORIGIN. This history
record is constructed locally from Begin; Genesis is never received through
the ordinary `BlockPersisted` data path. For every later block, the history
record comes from BlockProcessor's `PersistedBlock` delivery. The anchor seed
and later records provide the same non-null point and selected-parent shape.

These pending structures share one bounded capacity. A single
`HashMap<BlockHash, Vec<VspcChange>>` cannot represent multi-dependency
readiness and ordered candidate selection.

Resolve both source and destination before a notification becomes actionable.
An unresolved older notification must not head-block a later actionable one or
earn overlap merely because its raw destination hash is known. Distinct
incompatible resolved candidates require Resync.

Committed VSPC transitions are continuous and monotonic:

```text
ready[n].source == committed_sink
ready[n].source == ready[n - 1].destination
```

Competing arrivals need not be monotonic before selection. Materialization
history pruning is phase-specific:

```text
Before Catchup: prune entries with order < committed VSPC sink.
Catchup:        do not prune after synthetic or notification commits.
Catchup→Live:   prune entries with order < committed VSPC sink.
Live:           prune entries with order < sink after every successful commit.
```

The strict `<` comparison retains the sink entry required for added-only
source resolution. Do not remove buffered notifications solely because their
destination is below the sink without applying the phase-specific filtering
and overlap rules.

In Live, a rare network-delayed notification that cannot resolve after history
pruning may remain non-actionable in the bounded pending structure. It must not
block a later actionable crossing transition. Pending-capacity exhaustion
requires Resync.

## Readiness and materiality — settled

Any synthetic or notification change can commit, including during PreSeal,
only when both predicates hold:

```text
CHAIN_READY: ready.source == committed_vspc_sink
BLOCK_READY: ready.destination is BoundaryMaterialized
```

An actionable reorg calls
`ValidatedDbClient::resolve_materialized_ids(removed + added)` once. The
ordered, cache-first result preserves input positions, including repeats, and
distinguishes absent from boundary-identity members. Added-only changes derive
their source and member IDs from retained materialization history without that
database read.

Endpoint `VspcPoint` consensus order comes from `PersistedBlock` history. The
database batch resolves member IDs; it does not derive endpoint order or load
merge sets for the processor.

A block named directly in `added` or `removed` that is confirmed
nonmaterialized violates the retained-graph invariant and requests
`Require(Rebuild)`. The database can no longer be trusted against node state.
An identity-only member appearing only in an added block's merge set is an
outside-boundary reference and is ignored by coloring. These cases are not
equivalent.

The storage transaction validates source continuity, every direct chain
member's materiality, and vector consistency. Its complete persistence and
coloring behavior is defined by the
[atomic VSPC transaction](storage.md#atomic-vspc-transaction--settled).

## Catchup filtering, crossing, and overlap — settled

Catchup supplies an objective synthetic-sink lower bound:

```rust
struct VspcCatchup {
    synthetic_sink_lower_bound: VspcPoint,
}
```

Begin resets VspcProcessor's overlap flag and all run-local pending/history
state. Catchup begins overlap accounting. A notification destination below
the lower bound is a latecomer and is discarded without overlap credit.
Unresolved raw destinations and consensus-order comparisons alone never earn
credit.

While the coordinated pump supplies synthetic changes, VspcProcessor commits
that ordered stream with priority. Notifications may be consumed, resolved,
and considered for overlap, but cannot overtake an available synthetic
predecessor. A temporarily empty synthetic queue does not prove production has
ended and does not authorize a notification commit.

For committed synthetic sink `C`, classify each resolved notification in this
order:

1. Resolve `D = added.last()`. If `D.order <= C.order`, discard the
   notification under the lower-bound rules. Accepted retained history may
   prove the meeting and earn overlap credit. `D == C` is a discard, never an
   empty normalized change.
2. If `D.order > C.order` and `C.hash` occurs in `added`, discard `removed`
   and the `added` prefix through `C`. The necessarily nonempty added-only
   suffix has source `C` and is actionable after endpoint readiness.
3. Otherwise derive and resolve the original source. If it equals `C`, the
   original change is directly actionable.
4. Otherwise retain it pending and continue scanning later candidates.

The sink's occurrence in `removed`, a later destination, or a consensus-order
comparison cannot prove crossing. Resolve both endpoints of the resulting
actionable change before readiness.

A notification proved by accepted history to meet the committed synthetic
stream sets VspcProcessor's `AtomicBool` overlap flag. Filtered older
latecomers receive no credit. A duplicate transition from the notification
source is an invariant violation requiring Resync, not an idempotent discard.
ResyncEngine owns observation of the overlap flag and the global ordinary
eligibility predicate.

## Component-local Live transition — settled

At ordinary eligibility, the processing lifecycle stops and joins synthetic
VSPC production before sending VspcProcessor its existing exact-once `Live`
command. VspcProcessor then:

1. clears or discards queued synthetic input;
2. ignores later synthetic input for the rest of the session;
3. prunes history below the committed sink with strict `<`; and
4. makes retained and new actionable notifications authoritative.

This is VspcProcessor's existing Live phase, not an additional coverage phase,
terminal marker, checkpoint, or barrier. BlockProcessor can remain in Catchup
while the lifecycle performs body-tip coverage.

Available block material lets notification-driven sink advancement continue.
A notification whose needed block material has not arrived remains in
pre-resolution readiness while block processing and dependency resolution
continue. This waiting occurs before strict materiality resolution and does
not weaken the direct Rebuild fault when an actionable transition confirms a
nonmaterialized chain member.

If later block coverage exhausts its page budget, the lifecycle requests
Resync and deactivates the session. Recovery derives its next starting sink
from the database state actually committed by VspcProcessor; there is no
coverage checkpoint.

## Commit and graph publication — settled

For each ready transition, VspcProcessor invokes storage's atomic VSPC
transaction. Only definite commit advances its local committed sink and
history. Storage returns the destination `VspcPoint`; no caller-side
destination reconstruction or clone is required.

After definite commit, VspcProcessor publishes the corresponding
`VspcCommitted` update to the single ordered graph observer channel. The
[BlockProcessor delivery contract](block-processing.md#committed-block-delivery)
sends each newly materialized block's graph update before the `PersistedBlock`
that can make a VSPC transition ready. Therefore VspcProcessor cannot publish
a VSPC mutation ahead of its causal block updates.

Graph observer delivery is nonblocking for processing. Failure invalidates
ApiService's image under the [API contract](api.md), rather than rolling back
the committed VSPC transaction or requesting processing recovery.

The committed sink remains derived from materialized VSPC membership in
storage. VspcProcessor does not persist a separate sink or checkpoint.
