# VSPC processing

> Focused extraction; the current consolidated contract is
> [handoff-2026-09-20.md](handoff-2026-09-20.md), which prevails on conflicts.

## VSPC semantics and types — settled

Rusty-kaspa change ordering:

```text
removed: old sink backward toward common ancestor, ancestor excluded
added:   common-ancestor child forward to new sink
```

`VspcChange` never contains Genesis in added or removed. A resolved source may
be Genesis for the first transition.

Source and destination are derived:

```text
source =
    removed.first(),                       if removed is nonempty
    selected_parent(added.first()),         otherwise

destination =
    added.last()
```

Every admitted nonempty change must have nonempty `added`. A removed-only
change is a typed unsupported invariant fault; it has no invented fallback
destination. Its upstream reachability remains to verify. An empty V2 page is
synchronization information, not a VSPC event. A wholly empty change remains
deferred.

Pending changes are stored as `VspcChange` values directly. Ready representation:

```rust
struct ReadyVspcChange {
    source: VspcPoint,
    destination: VspcPoint,
    removed: Arc<[CompactId]>,
    added: Arc<[CompactId]>,
}
```

Destination consensus order is mandatory in `ReadyVspcChange` through
`VspcPoint`. There is no `ReadyAddedBlock`; StorageService loads merge sets in
the transaction.

Resolution handles both event-before-block and block-before-event. Use:

- one ordered synthetic FIFO;
- raw unresolved notifications keyed by destination hash;
- resolved candidates ordered by destination consensus order;
- pending ID -> pending change;
- missing hash -> waiting pending IDs;
- hash -> recent `PersistedBlock` history;
- `ConsensusOrder -> hash` for ordered pruning/history.

This dual index is intentional. A `HashMap<BlockHash, Vec<VspcChange>>`
alone loses multi-dependency and sequencing structure.
These pending structures share one capacity bound. Resolve both endpoints
before readiness: an unresolved old notification must not block a later
actionable one or earn overlap merely from its destination hash. Distinct
incompatible resolved candidates request Resync.

Committed VSPC sources and destinations are monotonic. For committed events:

```text
ready[n].source == committed_sink
ready[n].source == ready[n - 1].destination
```

Incoming competing candidates do not need to be monotonic before selection.
Before Catchup, prune history entries with order strictly less than the
committed destination. Do not prune during Catchup. On Catchup→Live and after
each Live commit, prune entries with order strictly less than the committed
destination, retaining the sink entry.

### VSPC readiness at the PP boundary

Any synthetic or notification change can be processed, even during `PreSeal`,
when both hold:

```text
CHAIN_READY: ready.source == committed_vspc_sink
BLOCK_READY: ready.destination is BoundaryMaterialized
```

Because retained PP-anticone blocks arrive before PP-future blocks that merge
them, nonmaterialized merge-set identities referenced by added blocks are
outside the retained boundary and can be ignored during coloring.

A nonmaterialized block named directly in `added` or `removed` violates the
invariants and directly requests `Require(Rebuild)`: the DB can no longer be
trusted against node state. An outside-boundary identity named only in an
added block's merge set is ignorable during coloring.

For an actionable reorg, call `ValidatedDbClient::resolve_materialized_ids`
once for `removed` followed by `added`. The ordered, cache-first result
preserves repeated inputs and distinguishes absent from identity-only
members. Added-only changes resolve from history and make no DB lookup.

### Catchup crossing and overlap

While the coordinated Catchup pump supplies synthetic changes, commit that
ordered stream with priority; a notification must not overtake an available
synthetic predecessor. A temporarily empty synthetic queue does not prove the
stream has ended. At ordinary eligibility, ResyncEngine stops/joins synthetic
production and sends the existing Live command. VspcProcessor then
clears/discards synthetic input for the rest of the session and begins normal
Live notification processing. For committed synthetic sink `C`, classify a
resolved notification in this order:

1. Resolve `D = added.last()`. If `D.order <= C.order`, discard the
   notification under the lower-bound rules and give overlap credit only when
   accepted history proves the meeting. `D == C` is a discard, not an empty
   normalized change.
2. If `D.order > C.order` and `C.hash` occurs in `added`, discard `removed`
   and the `added` prefix through `C`, then apply the necessarily nonempty
   added-only suffix sourced at `C`.
3. Otherwise, derive and resolve the original source. Apply the original
   change only if that source equals `C`.
4. Otherwise retain it pending and continue scanning later candidates.

A sink in `removed` or a mere order comparison does not establish
continuity. Resolve both endpoints of the resulting actionable change before
readiness. Keep unresolved notifications bounded and without overlap credit.

### Operation during block-coverage admission

Ordinary block/VSPC overlap at a complete GetBlocks-page boundary starts the
bounded block-coverage phase and VspcProcessor's component-local Live
transition; it does not authorize BlockProcessor Live or global `EnteredLive`.
ResyncEngine stops issuing VSPC V2 calls and producing new synthetic changes,
stops/joins that producer, and sends VspcProcessor its existing Live command.

No additional VSPC phase, terminal marker, barrier, or checkpoint is needed.
The component-local Live transition clears/discards queued synthetic input and
ignores later synthetic input for the rest of the session. VspcProcessor
starts committing retained/new actionable notifications under its normal Live
rules while BlockProcessor remains in Catchup. When all required block
material is available, the committed sink can keep advancing. Missing material
keeps the affected notification in pre-resolution readiness waiting while
block processing and dependency resolution continue. This does not weaken the
direct Rebuild fault when strict resolution of an actionable transition
confirms a nonmaterialized chain member.

On coverage-page cap exhaustion, `Require(Resync)` starts later recovery from
the sink actually committed in database state at that time.

### Atomic VSPC transaction

Storage validates that source equals current committed sink and that every
added/removed chain block is materialized, with no duplicates or intersection.
It loads added-block merge sets internally.

Conceptual updates:

- removed blocks: `is_in_vspc = false`, reset relevant color to Gray;
- added blocks: `is_in_vspc = true`;
- for each added block in order, color materialized blue merge-set members Blue
  and then materialized red members Red;
- ignore identity-only merge-set members;
- red application after blue resolves any overlap deterministically.

The whole change is one transaction. It returns the destination `VspcPoint`,
which naturally avoids a destination clone at the caller.

There is no dedicated committed-sink table. It is derived from materialized
blocks with `is_in_vspc = true`.
