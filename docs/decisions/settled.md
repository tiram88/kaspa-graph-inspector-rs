# Settled decision index

This file indexes accepted KGI v2 outcomes. The linked focused architecture is
the complete contract and rationale source; this index does not restate its
mechanics.

| Concern | Settled outcome | Current owner |
|---|---|---|
| Identity and materiality | Persistent hash identity is distinct from materialized block state; PP-boundary identities are permanent, and ORIGIN represents the selected parent outside the boundary without becoming a direct Genesis parent. | [Domain model](../architecture/domain-model.md), [storage](../architecture/storage.md) |
| Database lifecycle | `--initialize-db` acts only on Uninitialized state; atomic complete NodeMetadata creates network-bound Empty state; compatible Empty, Initialized, or Inconsistent data is retained; immutable network identity mismatch is rejected; processing-data replacement requires Rebuild with a pruning point; and deliberate schema replacement uses the one-shot administrative command or an idempotent declarative token. | [Storage](../architecture/storage.md) |
| Node capability | A processing session uses one validated RPC generation, RPC-discovered Genesis identity, normalized notifications, and normalized full-block RPC responses. | [NodeService](../architecture/node-service.md) |
| Block processing | Block admission, in-memory orphan topology, bounded dependency resolution, and definitely committed PP sealing are distinct responsibilities with owner-directed faults. | [Block processing](../architecture/block-processing.md) |
| VSPC processing | VSPC changes are normalized, resolved through ordered pending history, committed only when ready, and switch from synthetic priority to notification authority at component-local Live. | [VSPC processing](../architecture/vspc-processing.md) |
| Recovery lifecycle | Recovery strength is `Resync < Rebuild`; Resync and Rebuild share one pump, Genesis is intrinsically sealed with committed-sink RPC anchors, other boundaries use the accepted finalization threshold, Catchup starts through the rolling sink or retained fallbacks, and bounded fixed-tip coverage precedes global Live. | [Processing lifecycle](../architecture/processing-lifecycle.md), [block processing](../architecture/block-processing.md) |
| API publication | ApiService uses Reset, PostSeal, and Live publication controls, coherent GraphEpoch replacement, a complete level-bounded head cache, and bounded historical reads. | [API](../architecture/api.md) |
| Resource isolation | Processing has reserved capacity and priority; API pools, admission lanes, caches, and client work are bounded and degrade explicitly under saturation. | [Overview](../architecture/overview.md), [API](../architecture/api.md) |
| Web behavior | Browser state follows API epochs and cursors, uses stable hash identity, freezes fixed views, and recognizes Genesis by zero actual direct parents. | [Web](../architecture/web.md) |
| Verification | PUAR source analysis assesses upstream correctness assumptions against one pinned reference revision; executable evidence covers KGI-owned behavior, integration boundaries, and architecture acceptance scenarios. | [Verification](../architecture/verification.md) |

Later standalone accepted ADRs may be added to this directory and indexed here.
