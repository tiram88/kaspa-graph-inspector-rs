# Superseded designs

These designs were replaced by later accepted decisions. They are retained to
explain why older handoffs or implementation notes may use different terms.

| Superseded design | Current replacement | Current owner |
|---|---|---|
| Complete the initial scan before subscribing, then repair only from final VSPC. | Deliberate Catchup overlap followed by ordinary dependency and recovery handling for omissions that later become relevant. | [Processing lifecycle](../architecture/processing-lifecycle.md) |
| Create placeholder block rows at levels 0/1 and promote them later. | Permanent outside-boundary identities plus separately materialized retained blocks. | [Domain model](../architecture/domain-model.md), [storage](../architecture/storage.md) |
| Treat any identity-row existence as block materiality. | Separate `Absent`, `BoundaryIdentity`, and `Materialized` lookup states; apply `BoundaryMaterialized(block)` as a stronger retained-past predicate where required. | [Domain model](../architecture/domain-model.md) |
| Prune VSPC history through `sink` using `<=`. | Prune entries strictly below sink and retain the sink entry. | [VSPC processing](../architecture/vspc-processing.md) |
| Estimate Catchup with a large sink-anticone window, tenfold merge-set margin, or fixed X/Y page rule. | Rolling RPC sink marker with VSPC membership, a page-aware DAA threshold, and retained fallbacks. | [Processing lifecycle](../architecture/processing-lifecycle.md) |
| Allow only ResyncEngine reconciliation to request Rebuild. | A direct nonmaterialized VSPC chain member and a resolver-confirmed unavailable dependency also request Rebuild. | [Processing lifecycle](../architecture/processing-lifecycle.md) |
| Gate global Live on a fixed body-tip snapshot, materiality batch, and bounded extra GetBlocks pages. | PostSeal plus both overlap flags at a complete page boundary directly transitions both processors and global publication to Live. GetBlocks does not enumerate the whole retained body DAG, and KGI relies on ordinary dependency and recovery mechanisms when an earlier omission later becomes relevant. | [Processing lifecycle](../architecture/processing-lifecycle.md) |
| Use an `Auto` recovery mode that falls through from Resync to Rebuild. | Explicit `Resync < Rebuild` obligations and a distinct Supervisor-started Rebuild run. | [Processing lifecycle](../architecture/processing-lifecycle.md) |
| Defer the API outside KGI v2. | The in-process ApiService, publication controls, and Web-facing graph contract are part of v2. | [API](../architecture/api.md) |
