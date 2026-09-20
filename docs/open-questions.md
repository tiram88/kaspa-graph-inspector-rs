# Open questions

This file contains only unresolved or deliberately deferred implementation matters. None of these items weakens or reopens a settled architecture contract.

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
6. Concrete error enums and retry/backoff constants.
7. API endpoint URLs and final wire schema; graph response format selected by
   end-to-end benchmark, not by assumption.
8. The exact `MAX_WINDOW_DEPTH` (bounded by the settled 1000-level cache),
   HTTP/SSE budgets, adaptive fixed-view delay curve, and graph-delta history
   size.
9. Detailed historical-read cancellation/transaction mechanism around Reset;
   exact PostSeal/Live timing of publishing a complete new API image.
10. Metrics, tracing, operational endpoints, and deployment layout.
11. Exhaustive test matrix and parity fixtures against Go KGI/rusty-kaspa.
12. Shutdown timeouts and escalation policy.
13. Fine implementation details previously grouped under design point 10.4.4.
14. Whether a real fully empty `VspcChange` ever needs explicit support; a
    VSPC V2 empty page is already handled separately.
