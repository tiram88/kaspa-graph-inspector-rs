# KGI v2 architecture overview

> This document is a focused extraction of the authoritative 17 September 2026 architecture handoff. Its settled semantics are unchanged.

## Purpose and authority

This document transfers the settled architecture for the Rust rewrite of the
Kaspa Graph Inspector into the new `kaspa-graph-inspector-rs` repository.

The rewrite covers the KGI processing and API tiers. The existing Go KGI,
`rusty-kaspa`, and `simply-kaspa-indexer` are reference implementations, not
code to copy mechanically.

For the receiving chat:

1. Treat statements marked **settled** below as accepted design constraints.
2. Do not silently replace them with a simpler implementation.
3. Record future accepted architecture changes in versioned repository
   documentation or ADRs immediately.
4. Keep implementation details distinguishable from architectural contracts.
5. Flag conflicts between this document and source-code evidence rather than
   resolving them implicitly.

The old project handoff dated 8 September 2026 is superseded where this
document contains later decisions.

## Repository and chat workflow

The Rust repository is:

```text
/home/pool/dev/tiram88/kaspa-graph-inspector-rs
```

The intended collaboration split is:

- **Architecture chat**: owns normative architecture documents and ADRs. It
  does not implement production code unless explicitly asked.
- **Implementation chat**: implements accepted architecture, tests, and
  migrations. It must not invent architecture silently.
- **Review chat**: reviews stable commits/diffs against the accepted
  architecture. It is read-only unless explicitly asked to fix findings.

The repository is the durable source of truth; chat transcripts are not.

Recommended initial repository documents:

```text
AGENTS.md
docs/architecture/overview.md
docs/architecture/processing-lifecycle.md
docs/architecture/block-processing.md
docs/architecture/vspc-processing.md
docs/architecture/storage.md
docs/decisions/
docs/open-questions.md
docs/implementation-status.md
docs/reviews/
```

Normative precedence:

```text
accepted architecture and ADRs
    > implementation and tests
    > implementation-status and review notes
```

## System overview — settled

KGI v2 uses a tree of autonomous workers with control channels rather than a
single transient processing task.

```text
Supervisor
├── Arc<NodeClient>
├── Arc<StorageService>
└── Arc<ResyncEngine>
    ├── Arc<BlockProcessor>
    │   ├── OrphanManager
    │   └── DependencyResolver
    └── Arc<VspcProcessor>
```

Lifecycle control flows from parent to child. Reliable faults and milestones
flow from child to parent. Avoid strong-reference cycles.

`ResyncEngine` owns the processors. Using `Arc<Component>` rather than a thin
handle is acceptable as an implementation choice, provided lifecycle and
ownership contracts remain clear.

Notifications never pass through `ResyncEngine`:

```text
NodeClient::NotificationRouter
├── BlockAdded ---------> BlockProcessor notification channel
└── VirtualChainChanged -> VspcProcessor notification channel
```

Each processor aggregates its channels in one event loop, preserving local
sequential state transitions while allowing concurrent producers.

## Core identities and coordinates — settled

```rust
type SharedNodeBlock = Arc<RpcBlock>;

struct ConsensusOrder {
    blue_work: BlueWork,
    hash: BlockHash,
}

struct VspcPoint {
    consensus_order: ConsensusOrder,
    id: CompactId,
}

struct BlockCoordinate {
    level: u64,
    slot: u64,
}

struct MaterializedSyncAnchor {
    point: VspcPoint,
    blue_score: u64,
}
```

`ConsensusOrder` is ordered lexicographically by `(blue_work, hash)`.
`VspcPoint` conceptually extends `ConsensusOrder`; it must not duplicate the
block hash. Small explicit accessors such as `hash()`, `order()`, and `id()` are
fine. Do not use `Deref` to model this relationship.

`CompactId` is a positive incremental signed 64-bit database identifier.

The old schema terminology was renamed:

```text
height              -> level
height_group_index  -> slot
height_groups       -> levels
levels.size remains size
```

