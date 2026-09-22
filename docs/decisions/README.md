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
- Large sink-anticone capacity/window calculation and a fixed X/Y page rule
  for Catchup proximity/overlap: replaced by a rolling RPC sink marker
  established by the gapless VSPC pump and a page-aware DAA threshold. The
  global-maximum-position, normalized-length `< 3`, and empty-VSPC conditions
  remain independent fallbacks. The network-scaled page cap after ordinary
  Live eligibility is a body-tip coverage budget, not a Catchup trigger.
- Local or adjacent GetBlocks order decrease as a Catchup trigger: rejected.
  Only the position of the page's global maximum is relevant to the accepted
  order-based fallback.
- Dedicated recovery or a separate completeness gate for a GetBlocks response
  assembled across changing virtual views: rejected. The rolling sink is the
  normal Catchup path; a material omission exposed during strict pre-Catchup
  processing requests `Require(Resync)` through the existing fault path.
- Deriving a destination from a removed-only VSPC change: unsupported
  invariant fault; admitted nonempty changes require `added.last()`.
- Enabling local notification routing before both remote subscriptions have
  started: routing remains Disabled through activation, dropping callbacks
  until both starts succeed.
- Blanket rule that only ResyncEngine may request Rebuild: a nonmaterialized
  VSPC chain member or resolver-confirmed unavailable dependency requests it
  directly.
- No-unresolved-orphans condition for Live: unnecessary.
- Entering Live from overlap flags alone: superseded by the fixed body-tip
  coverage invariant. ResyncEngine stops producing synthetic VSPC changes at
  the ordinary eligibility boundary. An acknowledged VSPC freeze/barrier and
  special coverage checkpoint are rejected. VspcProcessor receives its
  existing Live command at that boundary while BlockProcessor remains in
  Catchup; no additional VSPC phase or synthetic-stream terminal marker is
  introduced.
- `Auto` recovery mode: removed; Resync failure explicitly requires Rebuild.
- Dedicated persisted VSPC checkpoint/sink table: derived from block state.
- Dedicated persisted PP identity by CompactId: DB PP is `(1, 0)`.
- `ReadyAddedBlock`: unnecessary; Storage loads merge sets transactionally.
- `ProcessingResources` wrapper: unnecessary.
