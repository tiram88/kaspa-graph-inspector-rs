# Open questions

This file contains only unresolved requirements or deliberately deferred
implementation matters. None of these items weakens or reopens a settled
architecture contract.

## Still open or deliberately deferred

The current contract is consolidated in
`docs/architecture/handoff-2026-09-20.md`. The following remain for later
analysis or implementation; none permits weakening a settled invariant:

1. Cargo workspace/crate/module layout.
2. Exact PostgreSQL client, migration framework, and concrete SQL types.
3. Exact capacities for processor channels, orphan memory, caches, and RPC
   concurrency.
4. Orphan occupancy threshold choice between approximately one quarter and one
   third.
5. Detailed Tokio draining/fairness implementation.
6. Remaining concrete error enums and retry/backoff constants. The typed
   synthetic removed-only VSPC cause and its maximum of three whole-attempt
   retries per validated RPC generation are already settled.
7. API endpoint URLs and final wire schema; graph response format selected by
   end-to-end benchmark, not by assumption.
8. The exact `MAX_WINDOW_DEPTH` (bounded by the settled 1000-level cache),
   HTTP/SSE budgets, adaptive fixed-view delay curve, and graph-delta history
   size.
9. Detailed historical-read cancellation/transaction mechanism around the
   Rebuild Reset barrier.
10. Exact metrics export/labels, tracing details, operational endpoints, and
    deployment layout. The required v2 metrics and processing-latency load
    acceptance are settled in the handoff.
11. Exhaustive test matrix and parity fixtures against Go KGI/rusty-kaspa.
12. Shutdown timeouts and escalation policy.
13. Fine implementation details previously grouped under design point 10.4.4.
14. Design a separate explicit administrative reset if needed; no persistent
    destructive `--reinitialize-db --yes` startup option.
15. Specify the recovery contract for `db_pp == Genesis`. Cover how an
    initialized Genesis-anchored DB is distinguished from Empty, Resync versus
    Rebuild eligibility before and after boundary sealing, the GetBlocks and
    VSPC starting anchors, and the resulting lifecycle milestones. Preserve
    the settled representation: Genesis has a non-null ORIGIN selected-parent
    identity, zero actual direct parents, and remains at `(level=1, slot=0)`.
