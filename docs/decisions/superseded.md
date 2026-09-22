# Superseded designs

These designs were replaced by later accepted decisions. They are retained to
explain why older handoffs or implementation notes may use different terms.

| Superseded design | Current replacement | Current owner |
|---|---|---|
| Complete the initial scan before subscribing, then repair only from final VSPC. | Deliberate Catchup overlap plus fixed-tip coverage before global Live. | [Processing lifecycle](../architecture/processing-lifecycle.md) |
| Create placeholder block rows at levels 0/1 and promote them later. | Permanent outside-boundary identities plus separately materialized retained blocks. | [Domain model](../architecture/domain-model.md), [storage](../architecture/storage.md) |
| Treat any identity-row existence as block materiality. | Explicit `IdentityOnly`, `BoundaryMaterialized`, and `StrictlyMaterialized` states. | [Domain model](../architecture/domain-model.md) |
| Estimate Catchup with a large sink-anticone window, tenfold merge-set margin, or fixed X/Y page rule. | Rolling RPC sink marker with VSPC membership, a page-aware DAA threshold, retained fallbacks, and a later coverage-page budget. | [Processing lifecycle](../architecture/processing-lifecycle.md) |
| Allow only ResyncEngine reconciliation to request Rebuild. | A direct nonmaterialized VSPC chain member and a resolver-confirmed unavailable dependency also request Rebuild. | [Processing lifecycle](../architecture/processing-lifecycle.md) |
| Enter global Live as soon as both overlap flags are true. | Ordinary eligibility first transitions VspcProcessor, then fixed-tip coverage gates BlockProcessor and global Live. | [Processing lifecycle](../architecture/processing-lifecycle.md) |
| Use an `Auto` recovery mode that falls through from Resync to Rebuild. | Explicit `Resync < Rebuild` obligations and a distinct Supervisor-started Rebuild run. | [Processing lifecycle](../architecture/processing-lifecycle.md) |
