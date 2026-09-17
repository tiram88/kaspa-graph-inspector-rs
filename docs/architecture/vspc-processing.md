# VSPC processing

> This document is a focused extraction of the authoritative 17 September 2026 architecture handoff. Its settled semantics are unchanged.

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
    added.last(),                           if added is nonempty
    selected_parent(removed.last()),        otherwise
```

Every dispatched change is nonempty. An empty V2 response is synchronization
information, not a VSPC event.

Pending/ready representation:

```rust
struct PendingVspcChange {
    change: VspcChange,
}

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

- pending ID -> pending change;
- missing hash -> waiting pending IDs;
- hash -> recent `PersistedBlock` history;
- `ConsensusOrder -> hash` for ordered pruning/history.

This dual index is intentional. A `HashMap<BlockHash, Vec<PendingVspcChange>>`
alone loses multi-dependency and sequencing structure.

Committed VSPC sources and destinations are monotonic. For committed events:

```text
ready[n].source == committed_sink
ready[n].source == ready[n - 1].destination
```

Incoming competing candidates do not need to be monotonic before selection.
After each commit, prune history through the committed destination while
retaining the current sink separately.

### VSPC readiness at the PP boundary

Any synthetic or notification change can be processed, even during unsealed
Rebuild cycle 1, when both hold:

```text
CHAIN_READY: ready.source == committed_vspc_sink
BLOCK_READY: ready.destination is BoundaryMaterialized
```

Because retained PP-anticone blocks arrive before PP-future blocks that merge
them, nonmaterialized merge-set identities referenced by added blocks are
outside the retained boundary and can be ignored during coloring.

A nonmaterialized block named directly in `added` or `removed` violates the
invariants and triggers recovery.

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

