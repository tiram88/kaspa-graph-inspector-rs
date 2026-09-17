# Open questions

This file contains only unresolved or deliberately deferred implementation matters. None of these items weakens or reopens a settled architecture contract.

## Still open or deliberately deferred

The architectural contracts above are settled; the following remain for later
analysis or implementation:

1. Cargo workspace/crate/module layout.
2. Exact PostgreSQL client, migration framework, and concrete SQL types.
3. Exact capacities for processor channels, orphan memory, caches, and RPC
   concurrency.
4. Orphan occupancy threshold choice between approximately one quarter and one
   third.
5. Detailed Tokio draining/fairness implementation.
6. Concrete error enums and retry/backoff constants.
7. Exact API-tier compatibility and migration design.
8. Metrics, tracing, operational endpoints, and deployment layout.
9. Exhaustive test matrix and parity fixtures against Go KGI.
10. Shutdown timeouts and escalation policy.
11. Fine implementation details previously grouped under design point 10.4.4.
12. Event-history retention sizes, while preserving the settled pruning and
    correctness semantics.

