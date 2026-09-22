# KGI v2 architecture map

This document defines the target ownership boundaries for the architecture
reorganization. It is a navigation and migration contract, not a replacement
for any behavioral contract.

## Authority during migration

Until the explicit authority cutover:

- [`handoff-2026-09-20.md`](handoff-2026-09-20.md) remains the consolidated
  normative contract;
- the existing focused documents remain subordinate where their wording
  differs from that handoff; and
- a newly extracted document is not authoritative merely because its target
  path appears below.

No handoff moves to history until every section has been dispatched, checked
against subsequent accepted decisions, and covered by the focused
architecture.

## Target focused documents

After cutover, the focused architecture documents collectively own the
current behavioral contract. Each contract has one owning document; other
documents link to it instead of restating it.

| Target document | Sole concern |
|---|---|
| `README.md` | Architecture authority, reading order, ownership boundaries, and navigation. It contains no system behavior. |
| `overview.md` | System boundary, component ownership, process and data-flow shape, and system-wide resource isolation. |
| `domain-model.md` | Shared identities and graph vocabulary: hashes, compact IDs, coordinates, consensus order, materiality, PP boundary, and common value types. |
| `node-service.md` | Node connection lifecycle, validated RPC generations, subscriptions, notification routing, and RPC normalization. |
| `storage.md` | Database lifecycle, schema and metadata, persistent identities, transactions, caches, and PostgreSQL behavior. |
| `block-processing.md` | BlockProcessor, OrphanManager, DependencyResolver, block admission, and PP-boundary sealing behavior. |
| `vspc-processing.md` | VSPC normalization, readiness, sequencing, coloring, and VSPC-specific behavior during recovery and Live. |
| `processing-lifecycle.md` | Supervisor, recovery intent, ResyncEngine, Resync/Rebuild preparation, the common pump, Catchup and Live admission, fault policy, retries, and teardown. |
| `api.md` | In-process graph publication, API epochs and Reset, snapshots, revisions, deltas, SSE, ETags, DAA navigation, and graph-window APIs. |
| `web.md` | Browser client behavior and presentation requirements. |
| `verification.md` | Required fixtures, integration scenarios, acceptance checks, and upstream assumptions. It references contracts without redefining them. |

## Ownership rules

The dispatch follows these rules:

1. A behavioral rule has exactly one focused owner.
2. Shared names and value semantics belong to `domain-model.md`; component
   behavior using them remains with the component.
3. A coordinating component owns cross-component ordering and lifecycle
   transitions. Component-local reactions remain with that component.
4. Storage owns transaction, durability, schema, and cache-publication
   semantics even when another component requests the operation.
5. `processing-lifecycle.md` owns recovery phase changes. For example, it owns
   entry into Catchup and Live, while `vspc-processing.md` owns how
   VspcProcessor behaves in those phases.
6. Verification requirements live in `verification.md` and point to the
   contract under test. They do not restate that contract as a second source.
7. Decision records capture outcome, status, and rationale. The complete
   current behavior is written in the owning architecture document in the
   same change that settles the decision.
8. Historical handoffs and audits provide provenance only and never resolve a
   conflict with current architecture.

## Handoff dispatch map

Every numbered section of the 20 September handoff has the following target.
Where a section contains more than one concern, the row states the exact
split. This table controls the migration work but does not change behavioral
authority.

| Handoff section | Target owner and dispatch rule | Migration status |
|---|---|---|
| §0, Reading rules and scope | `README.md` for architecture authority and navigation; `../README.md` for repository-wide document classes. Scope constraints on future work go to `../future-work.md`. | Pending |
| §1, System shape and ownership | `overview.md`. Shared type names introduced only as vocabulary go to `domain-model.md`. | Extracted; final verification pending |
| §2, Shared identities and graph vocabulary | `domain-model.md`. Persistence-specific enforcement and schema representation go to `storage.md`. | Extracted; final verification pending |
| §3, Lifecycle, intent, commands, and channels | `processing-lifecycle.md` for lifecycle, command direction, channel semantics, fault classification, retries, and milestones. Reusable identity/value definitions go to `domain-model.md`; service-specific reconnect rules go to `node-service.md` or `storage.md`. | Pending |
| §4, NodeService and validated RPC | `node-service.md`. | Extracted and reconciled; final verification pending |
| §5, StorageService lifecycle and DB bootstrap | `storage.md`. Supervisor reactions to StorageService state link to `processing-lifecycle.md`. | Extracted and reconciled; final verification pending |
| §6, Retained persistence model and cache | `storage.md`. Shared materiality vocabulary links to `domain-model.md`. | Extracted and reconciled; final verification pending |
| §7, Block materialization and PP boundary | `storage.md` owns materialization transactions, schema invariants, and cache publication; `block-processing.md` owns when BlockProcessor requests those operations and reacts to PP sealing. | Extracted and reconciled; final verification pending |
| §8, BlockProcessor, orphan topology, dependency resolution | `block-processing.md`. Cross-worker recovery dispositions link to `processing-lifecycle.md`. | Extracted and reconciled; final verification pending |
| §9, VSPC changes, readiness, history, and atomic coloring | `vspc-processing.md` owns sequencing and readiness; `storage.md` owns the atomic coloring transaction; shared VSPC value definitions go to `domain-model.md`. | Extracted and reconciled; final verification pending |
| §10, ResyncEngine preparation and common pump | `processing-lifecycle.md`. Node RPC normalization used by the pump links to `node-service.md`; component-local admission behavior links to `block-processing.md` and `vspc-processing.md`. | Pending |
| §11, Catchup, overlap, Live, and late transport messages | `processing-lifecycle.md` owns phase transitions, coverage admission, timing, and global Live entry. `block-processing.md` and `vspc-processing.md` own their local phase behavior. `node-service.md` owns routing and transport-message handling. | Pending |
| §12, Teardown and delivery/failure semantics | `processing-lifecycle.md` owns teardown order, barriers, owner-directed faults, and session completion. Component-specific draining duties remain in the relevant component document. | Pending |
| §13, In-process API and graph observer feed | `api.md`. Producer-side observer publication guarantees remain in the relevant processor document and are referenced by `api.md`. | Extracted; producer ownership reconciled, final verification pending |
| §14, API snapshot, revision, delta, SSE, and ETags | `api.md`. | Extracted; final verification pending |
| §15, Reset and recovery-time API availability | `api.md` owns Reset and publication behavior; `processing-lifecycle.md` owns when lifecycle milestones trigger those API operations. | Extracted; lifecycle deduplication and final verification pending |
| §16, DAA navigation and window API | `api.md`. Storage query semantics needed by these endpoints remain in `storage.md`. | Extracted; final verification pending |
| §17, Web behavior | `web.md`. Wire contracts consumed by the browser remain in `api.md`. | Extracted; final verification pending |
| §18, Resource isolation and scalability | `overview.md` owns the system-wide isolation model. Concrete component budgets and saturation behavior remain with each component; API budgets remain in `api.md`. | Pending |
| §19, Tests and verification obligations | `verification.md`, organized by the architecture owner being verified. Exact unresolved test-matrix breadth remains a deferred decision rather than a duplicate contract. | Pending |
| §20, Remaining implementation decisions and v2.1 boundary | Unresolved v2 architecture goes to `../decisions/open.md`; constrained implementation choices go to `../decisions/deferred.md`; execution order goes to `../implementation/sequence.md`; work outside v2 remains in `../future-work.md`. | Pending |
| §21, Rejected designs | `../decisions/rejected.md` or `../decisions/superseded.md`, according to whether the design was never accepted or was replaced after acceptance. Any current replacement behavior remains in its focused architecture owner. | Pending |

## Cutover conditions

The focused architecture becomes authoritative only after all of these are
true:

1. Every row in the dispatch map has been applied.
2. Each dispatched section has been checked against later accepted changes.
3. No focused document says that the handoff prevails on conflict.
4. No current contract exists only in the handoff.
5. Cross-references, `AGENTS.md`, and the repository README identify the new
   owners consistently.
6. A final review confirms that the reorganization changed document ownership
   without changing accepted behavior.

After that review, one explicit cutover change will make the focused files the
normative architecture and move the handoffs into `docs/history/handoffs/`.
