# Architecture decisions

Accepted ADRs belong in this directory. ADRs are normative and may amend the focused architecture documents only when the change is explicit.

The following rejected and superseded designs are already settled and must not be reopened implicitly.

## Explicitly rejected or superseded designs

- Subscribe only after the whole initial scan and then repair solely from final
  VSPC: the architecture now uses deliberate Catchup overlap.
- Claim that recursive dependency loading in Go KGI is depth-limited: false;
  `ProcessBlockAndDependencies` recursively covers arbitrary depth.
- Placeholder block rows at level 0/1 and later promotion: replaced by
  permanent outside-boundary identities plus materialized retained blocks.
- Waiters in the committed block index: unnecessary; BlockProcessor receives
  the ID from commit/dedup and publishes `PersistedBlock` asynchronously.
- One monolithic worker for blocks and VSPC: split processors with immediate
  hash-to-ID communication.
- Ordering VSPC solely by arrival: unsafe.
- Rewind/replay of recently processed VSPC: rejected; use chronological
  buffering, chain continuity, and resync.
- Treating row existence as materiality: replaced by enforced
  `BoundaryMaterialized` invariant.
- Explicit notification epochs: impossible to assign reliably and unnecessary
  with Begin/Catchup gates and lower bounds.
- Large sink-anticone capacity/window calculation for Catchup: replaced by
  GetBlocks monotonicity/underfill plus empty-VSPC fallback.
- No-unresolved-orphans condition for Live: unnecessary.
- `Auto` recovery mode: removed; Resync failure explicitly requires Rebuild.
- Dedicated persisted VSPC checkpoint/sink table: derived from block state.
- Dedicated persisted PP identity by CompactId: DB PP is `(1, 0)`.
- `ReadyAddedBlock`: unnecessary; Storage loads merge sets transactionally.
- `ProcessingResources` wrapper: unnecessary.

