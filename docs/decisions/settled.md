# Settled decision index

This file indexes accepted KGI v2 outcomes. The linked focused architecture is
the complete contract and rationale source; this index does not restate its
mechanics.

| Concern | Settled outcome | Current owner |
|---|---|---|
| Identity and materiality | Persistent hash identity is distinct from materialized block state; PP-boundary identities are permanent, and ORIGIN represents the selected parent outside the boundary without becoming a direct Genesis parent. | [Domain model](../architecture/domain-model.md), [storage](../architecture/storage.md) |
| Database lifecycle | `--initialize-db` acts only on Uninitialized state; compatible Empty or Initialized data is retained, network mismatch is rejected, and processing-data replacement requires an explicit Rebuild with a pruning point. | [Storage](../architecture/storage.md) |
| Node capability | A processing session uses one validated RPC generation, normalized notifications, and normalized full-block RPC responses. | [NodeService](../architecture/node-service.md) |
| Block processing | Block admission, in-memory orphan topology, bounded dependency resolution, and definitely committed PP sealing are distinct responsibilities with owner-directed faults. | [Block processing](../architecture/block-processing.md) |
| VSPC processing | VSPC changes are normalized, resolved through ordered pending history, committed only when ready, and switch from synthetic priority to notification authority at component-local Live. | [VSPC processing](../architecture/vspc-processing.md) |
| Recovery lifecycle | Recovery strength is `Resync < Rebuild`; Resync and Rebuild share one pump, enter Catchup through the rolling sink or retained fallbacks, and require bounded fixed-tip coverage before global Live. | [Processing lifecycle](../architecture/processing-lifecycle.md) |
| API publication | ApiService uses Reset, PostSeal, and Live publication controls, coherent GraphEpoch replacement, a complete level-bounded head cache, and bounded historical reads. | [API](../architecture/api.md) |
| Web behavior | Browser state follows API epochs and cursors, uses stable hash identity, freezes fixed views, and recognizes Genesis by zero actual direct parents. | [Web](../architecture/web.md) |
| Verification | Upstream assumptions, integration boundaries, and architecture acceptance scenarios have a required executable evidence baseline. | [Verification](../architecture/verification.md) |

Later standalone accepted ADRs may be added to this directory and indexed here.
