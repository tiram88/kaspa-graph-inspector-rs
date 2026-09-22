# Storage architecture

## Scope and ownership

This document owns database connectivity, bootstrap and migration, persistent
representation, transaction semantics, cache publication, and database read
contracts. Shared identity and materiality meanings belong to the
[domain model](domain-model.md). Processing documents own when storage
operations are requested and how their results affect worker state.

## StorageService lifecycle — settled

`StorageService` is a permanent autonomous worker:

```text
StorageService
    -> connect
    -> acquire the database ownership lock
    -> initialize or migrate when authorized
    -> validate schema, network binding, and processing state
    -> publish Arc<ValidatedDbClient>
```

Conceptual state and Supervisor-facing operations are:

```rust
enum StorageServiceState {
    Connecting,
    AwaitingInitialization,
    Ready(Arc<ValidatedDbClient>),
    Unavailable(StorageUnavailableReason),
    Rejected(StorageRejection),
    Stopped,
}

wait_until_usable() -> Result<Arc<ValidatedDbClient>, StorageWaitError>
initialize_if_uninitialized(
    network_id: NetworkId,
    genesis_hash: BlockHash,
) -> Result<(), StorageError>
shutdown() -> Result<(), StorageError>
```

`wait_until_usable()` waits through transient `Connecting`,
`AwaitingInitialization`, and `Unavailable` states.
`AwaitingInitialization` means StorageService has opened, locked, and inspected
a never-initialized database but cannot yet publish it as usable. The state
remains pending until initialization is authorized and a validated node
identity supplies the complete immutable network binding. Permanent
`Rejected`, `Stopped`, or closed service state is returned to the caller.
Service state outlives individual validated DB generations.

`ValidatedDbClient` represents exactly one validated connection-pool
generation and owns that generation's caches. A processing run receives one
exact client generation; StorageService never rebinds the client underneath
the run. A replacement generation starts with fresh caches.

Transient connection and validation failures enter `Unavailable` and retry
indefinitely with nominal delays:

```text
1s, 2s, 4s, 8s, 16s, 30s, 30s, ...
```

Each actual delay uses equal jitter from 50% through 100% of the nominal
delay. Every wait is shutdown-cancellable. Reset the sequence only after
StorageService has remained continuously Ready for 60 seconds. Network
binding mismatch and unsupported, newer, v1, partial, or unknown schema
states enter terminal `Rejected` and do not retry under unchanged
configuration.

A connection-level storage failure retires the validated generation and ends
the active processing session, but does not retroactively revoke independent
operations already in flight. Each operation reports its actual outcome:

- a definitely committed transaction remains successful and authoritative;
- a definite rollback is a failure; and
- a connection loss with an ambiguous commit outcome remains ambiguous and is
  never transparently retried.

All persistent mutations are transactional. Cache entries become visible
only after definite commit. No connection epoch, revocation check, cache
generation, or global operation-completion barrier is required.

### Transaction retries

Only PostgreSQL SQLSTATE `40001` (serialization failure) and `40P01` (deadlock
detected) authorize a local retry of a processing semantic transaction. Retry
the **complete transaction** at most three times, after nominal delays `10ms`,
`50ms`, and `250ms`, each with equal jitter from 50% through 100%.

Return every other definite failure immediately. Never retry a connection
loss during commit because its outcome is ambiguous. Exhaustion produces the
typed `Persistence(RetryExhausted)` fault. Its recovery-versus-Live
disposition belongs to the
[processing lifecycle](processing-lifecycle.md). No cache state is published
before definite commit.

### Internal concurrency

The storage implementation provides separate sequential materialization and
VSPC mutation lanes while allowing their transactions to overlap. Rebuild
gets exclusive mutation access after processor deactivation. Reads remain
concurrent. A shared mutation lock plus per-lane mutexes is one valid shape;
exact lock types remain an implementation choice.

## Database bootstrap and validation — settled

Startup distinguishes these states:

```text
Uninitialized
    no complete KGI schema and immutable network binding exists; this is a
    transient storage/service initialization condition, not a usable database

Empty
    a valid v2 schema has complete immutable (network_id, genesis_hash)
    binding and db_pp_blue_score = 0, with no PP or processing data

Initialized
    a compatible schema has coherent PP, PP score, and materialized VSPC sink

Inconsistent
    schema and binding are valid, but processing contents require Rebuild

Rejected
    network identity mismatch or unsupported, newer, v1, partial, or unknown
    schema
```

StorageService may connect, acquire its ownership lock, inspect contents, and
perform authorized preparatory work while the database is `Uninitialized`.
None of those actions turns it into `Empty`. Only one atomic initialization
transaction supplied with the validated node's `(network_id, genesis_hash)`
may cross the semantic `Uninitialized -> Empty` boundary and publish a usable
database. A crash rolls that transaction back to `Uninitialized`; it cannot
expose a partially bound `Empty` database. A structurally valid initialized
schema and binding with incoherent processing data is `Inconsistent`, while a
partial schema or binding is `Rejected`.

`--initialize-db` initializes only `Uninitialized`. It is idempotent for every
compatible existing database: retain `Empty`, `Initialized`, or `Inconsistent`
state without erasing data or changing its network binding. An `Inconsistent`
database remains usable only for Rebuild and is not misclassified as `Empty`.
Without the flag, first initialization requires interactive confirmation;
noninteractive startup fails with an actionable confirmation error.

An existing database is never silently rebound to the CLI network, the
validated node's Genesis, or reset. A mismatch in either immutable binding
field is rejected rather than repaired through Rebuild.
`--clear-db` requests processing-data Rebuild under the existing compatible
network binding. A persistent destructive `--reinitialize-db --yes` startup
option is forbidden; a separate explicit administrative reset remains
open in the [decision register](../decisions/open.md).

StorageService acquires a dedicated PostgreSQL session advisory lock before
initialization, migration, or validation and holds it throughout the
validated-client lifetime. It rechecks state under that lock before first
initialization. Losing the lock connection retires the client and active
processing session.

Supported older v2 schemas migrate forward through ordered transactional
migrations under the lock before client publication. Automatic down, online,
or in-place v1-to-v2 migration is forbidden.

The correctness metadata, called **node metadata**, is conceptually:

```rust
struct NodeMetadata {
    network_id: NetworkId,
    genesis_hash: BlockHash,
    db_pp_blue_score: u64,
}
```

`network_id` and `genesis_hash` form the immutable database network binding.
No field is nullable and a partially bound `NodeMetadata` is invalid.
`db_pp_blue_score` is zero in an `Empty` database and in an initialized
Genesis-anchored database. PP presence distinguishes those states. For any
other initialized database it is the retained PP's blue score.
`ValidatedDbClient` exposes the immutable `NodeMetadata` read from its exact
database generation for session binding checks.

The database PP is located by `(level=1, slot=0)`, never by assuming ID 1 or
the minimum ID. A last-known node server version is observational and is not
persisted as correctness metadata.

An initialized database whose PP hash equals `NodeMetadata.genesis_hash` is a
valid Genesis anchor only when all of these invariants hold:

- the PP is materialized at `(level=1, slot=0)`, is in VSPC, and
  `NodeMetadata.db_pp_blue_score` is zero;
- the committed materialized VSPC sink exists and is coherent;
- Genesis has ORIGIN as its non-null selected-parent identity and has zero
  actual direct parents; and
- ORIGIN is the only `BoundaryIdentity`.

An extra boundary identity or another violation of these processing
invariants classifies the database as `Inconsistent` and requires Rebuild.

## Persistent representation — settled

The [domain model](domain-model.md#identity-and-materiality-vocabulary--settled)
defines `Absent`, `BoundaryIdentity`, `Materialized`, and
`BoundaryMaterialized`. Storage represents identity separately from
materialized graph data:

```text
block_identifiers: BlockHash -> CompactId
blocks: materialized block data keyed by CompactId
```

Ordinary unresolved orphan hashes are never persisted as identifier-only
rows. Only references classified outside the retained PP boundary can become
permanent boundary identities, and such identities are never promoted into
`blocks`. PP bootstrap persists synthetic ORIGIN as one of these identities
when it is the PP's selected parent; `blocks.selected_parent_id` remains
non-null without inventing an actual Genesis parent.

The conceptual SQL schema is:

```sql
block_identifiers(
    id BIGINT PRIMARY KEY,
    hash BYTEA UNIQUE NOT NULL CHECK (octet_length(hash) = 32)
);

blocks(
    id BIGINT PRIMARY KEY REFERENCES block_identifiers(id),
    timestamp BIGINT NOT NULL,
    daa_score BIGINT NOT NULL,
    level BIGINT NOT NULL CHECK (level > 0),
    slot BIGINT NOT NULL CHECK (slot >= 0),
    selected_parent_id BIGINT NOT NULL REFERENCES block_identifiers(id),
    color ... NOT NULL,
    is_in_vspc BOOLEAN NOT NULL,
    blue_merge_set BIGINT[] NOT NULL,
    red_merge_set BIGINT[] NOT NULL,
    UNIQUE (level, slot)
);

levels(
    level BIGINT PRIMARY KEY CHECK (level > 0),
    size BIGINT NOT NULL CHECK (size > 0),
    daa_score BIGINT NOT NULL DEFAULT 9223372036854775807
);

parents(
    child_id BIGINT NOT NULL,
    parent_id BIGINT NOT NULL,
    child_level BIGINT NOT NULL,
    child_slot BIGINT NOT NULL,
    parent_level BIGINT NOT NULL,
    parent_slot BIGINT NOT NULL,
    PRIMARY KEY (child_id, parent_id),
    CHECK (child_level > 0),
    CHECK (child_slot >= 0),
    CHECK (parent_level >= 0),
    CHECK (parent_slot >= 0),
    CHECK (parent_level > 0 OR parent_slot = 0)
);
```

`parents.child_id` and `parents.parent_id` intentionally have no foreign keys.
The materialization transaction validates their identities and embeds both
endpoint coordinates. An outside-boundary parent uses the sentinel `(0,0)`.
`levels.size` is the number of allocated slots at the level.

`levels.daa_score = i64::MAX` means that the level has no current VSPC block.
New levels use this sentinel, not zero. At most one current VSPC block occupies
a level, but a reorg can leave an existing level without one.

There is no dedicated persisted VSPC checkpoint. The committed materialized
VSPC sink is derived as the maximum-ID materialized block with
`is_in_vspc = true` and returned with its ID, hash, and stored DAA score.

Per-block blue work and blue score are not stored. `db_pp_blue_score` is the
only persisted blue-score metadata. The
[processing lifecycle](processing-lifecycle.md) owns node-header enrichment
and validation when constructing a `MaterializedSyncAnchor`.

## Caches and identity resolution — settled

Moka is a candidate cache implementation; the architecture fixes cache
contents and publication rules rather than the library.

```rust
struct CachedIdentity {
    id: CompactId,
    materialized: bool,
}
```

Caches contain:

- `BlockHash -> CachedIdentity`;
- `CompactId -> BlockCoordinate`; and
- `CompactId -> MergeSets`.

Do not cache negative identity results, mutable colors, or VSPC membership.
Cold identity batches may left-join `blocks` to obtain the materialized bit.
Publish cache entries only after the corresponding definite commit.

`ValidatedDbClient::resolve_materialized_ids` takes one ordered hash batch and
returns one ID per input position, including repeated hashes. It resolves
cache hits, performs at most one SQL read for all misses, and never writes,
interns, or promotes identities. Absent and boundary-identity hashes produce
distinct typed errors; only materialized hashes succeed.

## Rebuild transaction — settled

The sole API that clears processing data is:

```rust
async fn rebuild_from_pruning_point(
    pp: SharedNodeBlock,
) -> Result<MaterializedSyncAnchor, DbError>;
```

The pruning point argument is mandatory. One transaction:

1. clears processing data and PP-derived processing metadata;
2. restarts identity allocation;
3. interns required PP-boundary identities;
4. materializes PP at `(level=1, slot=0)`;
5. marks PP in VSPC and initializes `levels`;
6. stores `db_pp_blue_score`; and
7. returns the resulting materialized anchor after commit.

The immutable `(network_id, genesis_hash)` binding and schema/migration state
survive. Rebuild atomically replaces only `db_pp_blue_score` with the supplied
PP's score. Cache reset and seed are published only after definite commit.
This is not a general clear primitive callable without a pruning point.

When the PP's selected parent is synthetic ORIGIN, the transaction creates
that permanent outside-boundary identity and uses its non-null ID. Genesis's
actual direct-parent list remains empty.

The [API Reset barrier](api.md#reset-and-recovery-time-availability--settled)
and the [processing lifecycle](processing-lifecycle.md) own when this operation
may start; storage owns the atomic replacement itself.

## Block materialization transaction — settled

```rust
struct BlockMaterialization {
    hash: BlockHash,
    selected_parent: BlockHash,
    direct_parents: Vec<BlockHash>,
    blue_merge_set: Vec<BlockHash>,
    red_merge_set: Vec<BlockHash>,
    timestamp: Timestamp,
    daa_score: u64,
    // other immutable KGI-used RPC fields, if required
}

enum ReferencePolicy {
    AllowBoundaryIdentities,
    RequireMaterialized,
}

struct MaterializeBlockOutcome {
    id: CompactId,
    coordinate: BlockCoordinate,
    inserted: bool,
}
```

`BlockMaterialization` is hash/consensus-level input. It contains no DB ID,
level, or slot. Storage owns transactional ID resolution, coordinate
allocation, initial color, and persistence. Only level-zero direct parents
from `RpcBlock.header.parents_by_level` enter it.

For every materialization, intern hashes in this order:

1. red merge-set hashes;
2. blue merge-set hashes;
3. direct-parent hashes;
4. selected-parent hash; and
5. the block's own hash.

Deduplicate by first occurrence within this sequence. Validate:

- no contradictory self-reference;
- unique direct parents;
- selected parent occurs among direct parents for ordinary non-Genesis
  materialization;
- boundary materiality for references under the selected policy;
- an already materialized own hash as a dedup outcome returning its ID and
  coordinate; and
- an own hash already classified as a permanent boundary identity as an
  invariant violation.

`RequireMaterialized` requires every reference to be materialized and creates
no boundary identities. `AllowBoundaryIdentities` may intern missing
references as permanent outside-boundary identities, but it always
materializes the incoming block. Ordinary unresolved orphans never use the
permissive policy merely to persist missing hashes.

The PP bootstrap path handles ORIGIN separately: its non-null selected-parent
ID may name ORIGIN even though ORIGIN is not an actual direct parent. Genesis
has zero actual direct parents. This exception does not apply to ordinary
materialization.

Coordinate allocation is:

```text
no materialized direct parent:
    level = 1
    slot = next allocated slot at level 1

one or more materialized direct parents:
    level = max(materialized-parent levels) + 1
    slot = next allocated slot at that level
```

PP occupies `(level=1, slot=0)`. Retained PP-anticone roots with no
materialized parents occupy level 1 slots 1+. Other retained PP-anticone
blocks can have materialized anticone parents and follow the normal
parent-derived rule. "PP-anticone root" names only the first category.

Slot allocation is atomic, conceptually:

```sql
INSERT INTO levels(level, size, daa_score) VALUES ($1, 1, $no_vspc)
ON CONFLICT(level)
DO UPDATE SET size = levels.size + 1
RETURNING size - 1;
```

A newly inserted block starts `Gray` and outside VSPC. Parent rows contain
each actual materialized coordinate or the outside-boundary sentinel `(0,0)`.
Normal block materialization leaves the level DAA score at its sentinel.

On successful insert or dedup, return `MaterializeBlockOutcome`. Processing
owns publication of any resulting `PersistedBlock` and the PP-boundary phase
transition.

## Atomic VSPC transaction — settled

Storage requires the supplied source to equal the currently committed sink.
Every block directly named in `removed` or `added` must be materialized; the
vectors contain no duplicates or intersection. Storage loads each added
block's merge sets internally.

The transaction applies, in order:

1. for removed blocks, set `is_in_vspc = false` and reset the relevant color
   to `Gray`;
2. for added blocks, set `is_in_vspc = true`;
3. for every added block in order, color materialized blue merge-set members
   `Blue`, then materialized red merge-set members `Red`; and
4. update every affected `levels.daa_score` to its final current-VSPC score or
   the `i64::MAX` sentinel after all removals and additions.

An identity-only merge-set member outside the retained boundary is ignored.
Applying red after blue resolves any merge-set overlap deterministically. A
remove/add sequence may temporarily alter and then restore a level score; only
the final value is persisted.

The whole change commits atomically and returns the destination `VspcPoint`.
Storage does not return a separate list of level-score changes.

## Historical read contracts — settled

For valid query `0 <= q < i64::MAX`, the indexed database DAA-floor lookup
selects the greatest current VSPC score not exceeding `q`, breaking ties by
the highest level:

```sql
SELECT level
FROM levels
WHERE daa_score <= $1
ORDER BY daa_score DESC, level DESC
LIMIT 1;
```

The sentinel is excluded naturally because no valid query reaches
`i64::MAX`. If no retained floor exists, return an explicit no-retained-match
result. A query beyond the current VSPC DAA resolves to the current VSPC
level. Resolve a historical anchor and read its graph window in one consistent
database transaction, never from different revisions.
