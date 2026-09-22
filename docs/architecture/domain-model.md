# Shared domain model

## Scope and ownership

This document owns shared identities and graph vocabulary used by more than
one component. It defines what the values mean. Storage owns their persistent
representation and validation; processing documents own how workers use them.

Rust declarations are conceptual contract sketches. They do not fix crate or
module layout.

## Shared value types — settled

```rust
type SharedNodeBlock = Arc<RpcBlock>;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
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

`ConsensusOrder` sorts lexicographically by `(blue_work, hash)`. `VspcPoint`
contains that order and must not duplicate the block hash. Explicit `hash()`,
`order()`, and `id()` accessors are allowed; `Deref` must not model the
relationship.

`CompactId` is an incremental positive signed 64-bit internal identifier. It
is private to storage and processing and is not a public block identity.

`BlockCoordinate` is a database-local display coordinate. Slot allocation
order is not canonical across instances: the same block hash may have
different coordinates in independently allocated databases. A rebuilt
pruning point occupies `(level=1, slot=0)`.

## Coordinate terminology — settled

KGI v2 uses these names:

```text
height              -> level
height_group_index  -> slot
height_groups       -> levels
levels.size remains size
```

`levels.size` denotes the number of allocated slots in a level, not a
horizontal width.

## Identity and materiality vocabulary — settled

Block identity and block materiality are distinct concepts:

```text
Absent
    no persistent identity exists for the hash

BoundaryIdentity
    a persistent identity exists for a reference classified outside the
    retained pruning-point boundary, but no materialized block exists

Materialized
    persistent block data and a BlockCoordinate exist for the identity
```

The shared lookup result is conceptually:

```rust
enum BlockPresence {
    Absent,
    BoundaryIdentity { id: CompactId },
    Materialized {
        id: CompactId,
        coordinate: BlockCoordinate,
    },
}
```

An identifier row or `CompactId` alone does not prove materiality.
`BoundaryMaterialized(block)` is the stronger predicate:

> The block is materialized and its retained DAG past is fully materialized
> according to pruning-point-boundary semantics.

Ordinary unresolved orphan hashes are not boundary identities. They remain
transient processing state. A permanent `BoundaryIdentity` is never promoted
to `Materialized`; attempting to materialize it is an invariant violation.
Storage owns the persistent representation of these states, and
BlockProcessor owns enforcement of the retained-past invariant.

## Pruning-point boundary and ORIGIN — settled

The retained pruning-point boundary separates materialized retained graph
state from referenced history that KGI deliberately does not retain. A
reference outside that boundary can have a permanent identity without a
materialized block or drawable graph edge.

ORIGIN is a synthetic outside-boundary identity used when the pruning point's
selected parent is ORIGIN. This preserves a non-null selected-parent identity,
including for Genesis. ORIGIN is not an actual direct parent. A materialized
Genesis has zero actual direct parents; a pruned non-Genesis pruning point can
have actual direct parents even when none has a visible retained edge.

Selected-parent identity, actual direct-parent membership, retained edge
visibility, and materiality are therefore separate properties and must not be
inferred from one another.

## VSPC value types — settled

`VspcChange` is the normalized hash-level transition shared by NodeService,
the recovery pump, and VspcProcessor:

```rust
struct VspcChange {
    removed: Arc<[BlockHash]>,
    added: Arc<[BlockHash]>,
}
```

After endpoint and member resolution, VspcProcessor uses:

```rust
struct ReadyVspcChange {
    source: VspcPoint,
    destination: VspcPoint,
    removed: Arc<[CompactId]>,
    added: Arc<[CompactId]>,
}
```

`destination` carries mandatory consensus order through `VspcPoint`.
`ReadyVspcChange` contains the IDs and endpoint order required for readiness
and sequencing. Storage loads added-block merge sets inside its transaction.
