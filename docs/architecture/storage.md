# Storage and block materialization

> Focused extraction; the current consolidated contract is
> [handoff-2026-09-20.md](handoff-2026-09-20.md), which prevails on conflicts.

## Storage lifecycle — settled

`StorageService` is also a permanent autonomous lifecycle worker, symmetrical
with `NodeService`:

```text
StorageService
    -> connect and validate schema/configuration
    -> publish Arc<ValidatedDbClient>
```

Supervisor-facing APIs are conceptually:

```rust
wait_until_usable() -> Result<Arc<ValidatedDbClient>, StorageWaitError>
shutdown() -> Result<(), StorageError>
```

`ValidatedDbClient` is a session-scoped capability. It owns its pool and
caches. A processing session uses one exact storage generation; it is never
rebound underneath a running session.

Startup distinguishes Uninitialized (no recognized KGI schema or binding),
Empty (valid v2 schema bound to the exact CLI network but no processing data),
Initialized (coherent PP, score, and VSPC sink), and inconsistent or
unsupported state. `--initialize-db` initializes only Uninitialized and is
idempotent for a compatible Empty or Initialized DB. Without it, first
initialization requires interactive confirmation; noninteractive startup
fails with an actionable confirmation error. Network mismatch, v1, unknown,
newer, and partial schemas are rejected rather than rebound or silently
reset. Structurally valid but inconsistent processing contents request
Rebuild, distinct from Empty. `--clear-db` requests processing-data Rebuild
under the existing network binding. A persistent destructive
`--reinitialize-db --yes` startup flag is forbidden; separate administrative
reset is deferred.

StorageService holds a dedicated PostgreSQL session advisory lock before
initialization, migration, or validation and throughout the validated client
lifetime. Recheck state under the lock before first initialization. Losing
the lock connection retires the client and active processing session.
Supported older v2 schemas migrate forward in ordered transactions under
the lock; no automatic down, online, or v1-to-v2 migration is permitted.
API status/info relays the currently validated node server version; persisting
a last-known version is not a correctness prerequisite.

Any connection-level storage failure terminates the current processing
session, but does not retroactively revoke in-flight operations. An operation
reports its actual outcome:

- a committed transaction remains successful and authoritative;
- a definite rollback is a failure;
- an ambiguous connection-loss outcome remains ambiguous and is not retried
  transparently.

All persistent mutations are transactional. Caches are published only after
commit. A replacement validated generation starts with fresh caches.

No connection epoch, revocation check, cache generation, or global
operation-completion barrier is required.

Suggested internal concurrency:

- shared mutation `RwLock`;
- materialization lane mutex;
- VSPC lane mutex;
- rebuild takes exclusive mutation access after processor deactivation;
- reads remain concurrent;
- BlockProcessor and VspcProcessor transactions may run concurrently, while
  each lane remains sequential.


## Persistent block identity and materiality — settled

Identity and materiality are distinct:

```text
block_identifiers: BlockHash -> CompactId
blocks: materialized block data keyed by CompactId
```

Fundamental rule:

> Ordinary unresolved orphan hashes remain only in memory. They are not
> committed as identity-only rows. Persistent identity-only rows are created
> only for references classified outside the retained PP boundary, and they
> are never later promoted into `blocks` rows.

Attempting to materialize a permanent boundary identity later is an invariant
violation.

The stronger materiality invariant is:

> `BoundaryMaterialized(block)` implies that its retained DAG past is fully
> materialized according to PP-boundary semantics.

The existence of a database row or CompactId alone is insufficient.

PP bootstrap interns synthetic ORIGIN as a permanent outside-boundary
identity when the PP uses it as selected parent. The non-null
`blocks.selected_parent_id` then references ORIGIN, including for Genesis;
ORIGIN is not an actual direct parent. A present Genesis has zero actual
direct parents, which the API preserves independently of visible edges. This
marker requires neither a persisted network Genesis hash nor a dedicated
Genesis-hash API endpoint.

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

## Retained database schema — settled

Conceptual schema:

```sql
block_identifiers(
    id   BIGINT GENERATED ... PRIMARY KEY,
    hash BYTEA UNIQUE NOT NULL CHECK (octet_length(hash) = 32)
)

blocks(
    id                 BIGINT PRIMARY KEY REFERENCES block_identifiers(id),
    timestamp          ... NOT NULL,
    daa_score          ... NOT NULL,
    level              ... NOT NULL CHECK (level > 0),
    slot               ... NOT NULL CHECK (slot >= 0),
    selected_parent_id BIGINT NOT NULL REFERENCES block_identifiers(id),
    color              ... NOT NULL,
    is_in_vspc         BOOLEAN NOT NULL,
    blue_merge_set     BIGINT[] NOT NULL,
    red_merge_set      BIGINT[] NOT NULL,
    UNIQUE(level, slot)
)

levels(
    level ... PRIMARY KEY CHECK (level > 0),
    size  ... NOT NULL CHECK (size > 0),
    daa_score BIGINT NOT NULL DEFAULT 9223372036854775807
)

parents(
    child_id    BIGINT NOT NULL,
    parent_id   BIGINT NOT NULL,
    child_level ... NOT NULL CHECK (child_level > 0),
    child_slot  ... NOT NULL CHECK (child_slot >= 0),
    parent_level ... NOT NULL CHECK (parent_level >= 0),
    parent_slot  ... NOT NULL CHECK (parent_slot >= 0),
    PRIMARY KEY(child_id, parent_id),
    CHECK (parent_level > 0 OR parent_slot = 0)
)
```

`parents.child_id` and `parents.parent_id` deliberately have no foreign keys.
The materialization transaction validates referenced identities and embeds the
coordinates needed by visualization. Outside-boundary parents use `(0, 0)`.
The API filters `parent_level > 0` for visible DAG edges.

Per-block blue work and blue score are not stored. During Resync, storage
returns the committed materialized VSPC sink's ID, hash, and DAA score;
ResyncEngine validates an exact no-transactions GetBlock response and obtains
the sink's blue work and blue score from its header. `db_pp_blue_score` remains
the only persisted blue-score metadata. The DB PP itself is fetched by
`(1, 0)`.
`levels.daa_score = i64::MAX` means no VSPC member; new levels start with
this sentinel. At most one current VSPC member occupies a level, and a reorg
can leave that level empty. Valid DAA queries satisfy `0 <= q < i64::MAX`;
the public floor lookup selects greatest score `<= q`, then highest level on
a tie. Level and window reads use one coherent revision.

## Storage caches — settled

Moka is a suitable candidate.

```rust
struct CachedIdentity {
    id: CompactId,
    materialized: bool,
}
```

Caches:

- `BlockHash -> CachedIdentity`;
- `CompactId -> BlockCoordinate`;
- `CompactId -> MergeSets`.

Do not cache negative identity results. Mutable colors and VSPC membership are
not cached. Cold identity batches use a left join against `blocks` to obtain
the materialized bit efficiently. Cache publication occurs after commit only.

`ValidatedDbClient::resolve_materialized_ids` takes one ordered batch of
hashes and returns one ID per input position, including repeats. It checks
cache hits, uses at most one SQL read for misses, and never writes, interns,
or promotes identities. Missing and identity-only hashes produce distinct
typed errors; only materialized hashes succeed. Actionable VSPC reorgs pass
`removed` followed by `added` in this single batch. Added-only changes use
VSPC history without a DB lookup.

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

`BlockMaterialization` stays in hash/consensus terms. Storage owns CompactId
resolution, coordinate allocation, initial colors, and atomic persistence.

KGI uses only level-zero/direct parents from
`RpcBlock.header.parents_by_level`.

Universal hash interning order for every materialization:

1. red merge-set hashes;
2. blue merge-set hashes;
3. direct parent hashes;
4. selected-parent hash;
5. the block's own hash.

Deduplicate by first occurrence before interning.

Validation includes:

- no self-parent/reference contradiction;
- direct parents are unique;
- selected parent is a direct parent for ordinary non-Genesis blocks;
- an already materialized own hash is a dedup outcome;
- an identity-only own hash is an invariant violation.

Strict policy requires every referenced hash to be materialized and creates no
boundary identities. Permissive policy may create permanent identity-only rows
for missing references, then materializes the block itself.
The PP bootstrap path handles ORIGIN separately, allowing a non-null selected
parent outside the actual direct-parent list. Genesis has no actual direct
parents. Ordinary materialization does not use that exception.

Coordinate rule:

```text
zero materialized parents:
    level = 1
    slot = next slot at level 1

one or more materialized parents:
    level = max(materialized-parent levels) + 1
    slot = next slot at that level
```

PP is always `(level = 1, slot = 0)`. Retained PP-anticone roots occupy
`(level = 1, slot = 1+)`. Other retained PP-anticone blocks follow the normal
parent-derived rule.

Allocate slots atomically, conceptually:

```sql
INSERT INTO levels(level, size, daa_score) VALUES ($1, 1, $no_vspc)
ON CONFLICT(level)
DO UPDATE SET size = levels.size + 1
RETURNING size - 1;
```

Insert new blocks initially as `Gray` and not in VSPC. Parent rows contain the
actual materialized coordinate or boundary sentinel `(0, 0)`.

After successful commit, BlockProcessor publishes:

```rust
struct PersistedBlock {
    point: VspcPoint,
    selected_parent: BlockHash,
}
```

Every `PersistedBlock` is non-Genesis and therefore has an unconditional
selected parent. Genesis is handled only by the special PP/bootstrap path.

The atomic VSPC transaction updates affected `levels.daa_score` rows to
their final VSPC score or sentinel after all removals and additions. It may
leave a level without a VSPC member. Normal block materialization does not
set a real level DAA score.
