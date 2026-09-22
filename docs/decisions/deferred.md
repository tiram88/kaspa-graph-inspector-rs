# Deferred implementation decisions

These choices are deliberately left to implementation work. Implementations
must stay within the linked settled architecture and record choices where they
become durable project constraints.

1. Cargo workspace, crate, and module layout.
2. PostgreSQL Rust client, migration framework, and concrete SQL types.
3. Exact capacities for processor channels, orphan and VSPC pending memory,
   caches, DependencyResolver and RPC concurrency, delta history, and HTTP
   work. Local RPC scheduling and batching remain implementation choices only
   where the focused architecture does not fix request boundaries or batch
   semantics.
4. Orphan occupancy threshold within the settled range of approximately one
   quarter through one third.
5. Detailed Tokio fairness and drain mechanics.
6. API endpoint URLs and final wire schema. Select the graph response format
   from an end-to-end benchmark across server construction and serialization,
   compression, transfer, browser decoding, and graph-model construction.
7. Exact `MAX_WINDOW_DEPTH` within the settled 1000-level cache bound, HTTP and
   SSE budgets, adaptive fixed-view delay curve, and graph-delta history size.
8. Detailed historical-read cancellation and transaction mechanism around the
   acknowledged Rebuild Reset barrier.
9. Exact metrics export and labels, tracing, operational endpoints, and
   deployment layout. Required v2 observability and processing-latency
   acceptance remain settled.
10. Exhaustive parity matrix and additional fixtures beyond the required
    [verification baseline](../architecture/verification.md).
11. Shutdown timeouts and escalation policy.
Unlisted code-level choices remain implementation details only while they
preserve every settled contract and do not resolve an item in
[open.md](open.md) implicitly.
