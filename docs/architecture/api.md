# API architecture

## Scope and ownership

This document owns ApiService, graph publication, the `HeadGraphCache`, API
resource bulkheads, and the public graph API contract. The
[processing lifecycle](processing-lifecycle.md) owns when recovery milestones
send API controls. [Storage](storage.md) owns database transaction and query
implementation. Web client behavior belongs to the
[Web architecture](web.md).

## In-process API and graph observer feed — settled

KGI v2 includes an in-process `ApiService` and a complete bounded-by-level
`HeadGraphCache` (HGC), rather than directing each head request to expensive
PostgreSQL graph queries. The [system overview](overview.md#resource-isolation-and-scalability--settled)
owns deployment evolution and the single-writer constraint.

Processors send `BlockCommitted`/`VspcCommitted` through **one ordered,
bounded graph-update channel**, after their respective DB commits. The
[BlockProcessor delivery contract](block-processing.md#committed-block-delivery)
and [VspcProcessor producer contract](vspc-processing.md#commit-and-graph-publication--settled)
establish causal order, so a VSPC update cannot reach this channel ahead of
blocks it depends on.

The observer channel is nonblocking from the processing viewpoint:
failed/full delivery sets an out-of-band invalid flag; ApiService stops
publishing deltas and reloads from DB. No processing recovery or producer
sequence number is needed for observer-only continuity. The invalid flag also
catches loss of the **last** update that no later sequence number could
expose.

Conceptual committed payloads (field names can change, semantic content
must not):

```rust
struct GraphParent {
    hash: BlockHash,
    coordinate: Option<BlockCoordinate>, // None at outside-PP boundary
    level_size: Option<u64>,              // needed for external endpoint
    is_selected: bool,
}
struct BlockCommitted {
    hash: BlockHash,
    coordinate: BlockCoordinate,
    timestamp: Timestamp,
    daa_score: u64,
    direct_parents: Arc<[GraphParent]>,
    blue_merge_set: Arc<[BlockHash]>,
    red_merge_set: Arc<[BlockHash]>,
}
struct VspcCommitted {
    source: BlockHash,
    destination: BlockHash,
    removed: Arc<[BlockHash]>,
    added: Arc<[BlockHash]>,
}
```

The parent payload contains actual direct parents only. For every non-Genesis
block, its selected parent is one actual direct parent and appears exactly once
with `is_selected = true`; its coordinate can be `None` at the outside-PP
boundary. Genesis has an empty `direct_parents` payload. Synthetic ORIGIN is
Genesis's persisted selected-parent identity, but it is never inserted into
`direct_parents` and never becomes a public graph edge endpoint.

The payload therefore carries direct-parent coordinates and, for non-Genesis
blocks, the selected-parent coordinate without duplication. Updates for blocks
outside current HGC still need examination: a new block can increase
`levels.size` at an external parent endpoint used by an edge crossing the cache
boundary. The size of an external level may be seeded by an incoming child's
parent data and then updated monotonically when later blocks occupy it.

ApiService applies VSPC source/destination continuity:

```text
incoming.source == cache.committed_vspc_sink
```

If not, invalidate/reload HGC. From the removed/added vectors and cached
block metadata, ApiService independently updates cached VSPC membership,
colors, and level DAA scores, then publishes **one atomic graph revision**.
Processing/storage need not return a list of DAA-score changes or track them
for the API. Only actual final level-score changes matter: a remove/add pair
may temporarily change then restore a level's original score. An added
block's merge-set hash absent from complete HGC levels has no impact on
retained levels by definition; ignore it rather than invalidating cache.
Do not confuse this with a required chain block missing from cache when its
effect on a retained level cannot be determined; that case requires reload.

`MAX_CACHE_DEPTH = 1000` complete levels. Define a separate
`MAX_WINDOW_DEPTH <= MAX_CACHE_DEPTH`; its exact value remains deferred in the
[decision register](../decisions/deferred.md).
No arbitrary maximum block count may truncate a retained level: **every**
block and relevant edge endpoint for each cached level is available. Every
windowed endpoint caps requested depth to `MAX_WINDOW_DEPTH` and reports
the effective range. An oversized `/graph/head` request cannot fall back to
DB; it is capped.

The v1 edge rule remains: emit **every materialized edge whose span
intersects the requested window**, including a parent outside the response.
At the head, every such edge has its child in HGC, while its parent can be
outside. Retain cached child-to-parent records, parent hash/coordinate, and
the external parent's level size. Do **not** require both endpoint blocks
inside the requested window, nor omit a crossing edge. PP-boundary sentinel
edges at level 0 are not normal visible materialized edges.

`CompactId` is private to storage/processing. Each HTTP graph response has
its own small numeric references and an included local ID-to-hash dictionary
covering **all** hashes it references, including off-window parent endpoints
and merge-set members. These local IDs are not persistent across responses or
instances; coordinates may diverge across independently allocated DBs.

The public block projection preserves its **actual direct-parent list** even
when some parents are outside the response or PP boundary and have no
drawable edge. A materialized Genesis is recognized from its empty actual
direct-parent list. The public projection and Web client do not need to expose
or consult the persisted `NodeMetadata.genesis_hash`, and no dedicated
Genesis-hash API endpoint is required.

## Snapshot, revision, delta, SSE, and ETags — settled

```rust
struct HeadGraphCoverage {
    retain_from_level: u64,
    head_level: u64,
}
```

`HeadGraphCoverage` describes the complete HGC range at one published
revision. Every HGC-backed snapshot and every delta carries the coverage of
its target revision. `Delta(a,b)` therefore carries revision `b`'s coverage.
The invariant `retain_from_level <= head_level` holds, and every materialized
block in that inclusive range is present in HGC. This coverage is the complete
HGC range, independently of the narrower effective range selected for a
particular graph response.

External parent endpoints below `retain_from_level` remain reference-only graph
metadata needed by crossing edges; they do not extend HGC coverage. A delta
whose target boundary advances may omit block and level contents that have
left HGC, but it retains the endpoint metadata required by every crossing edge
in the target image. Within one GraphEpoch, `retain_from_level` never
decreases and `head_level` never decreases.

ApiService begins buffering graph updates **before** a consistent DB snapshot
read. The snapshot includes complete retained levels/blocks, external edge
endpoint coordinates and level sizes, current colors/VSPC membership,
committed VSPC sink, and current DB data needed for DAA resolution. Replay
buffered block updates idempotently without overwriting snapshot colors;
skip VSPC prefix already represented by the snapshot, then require exact
source/sink continuity. On each new processing session, reload because the
previous session may have committed data without publishing its observer
update. Every reload creates a new **GraphEpoch**: this is an API publication
continuity epoch, not a DB, node, notification, or CompactId epoch. An old
coherent image can remain served as explicitly **stale** until a coherent
replacement is ready, but it has no active deltas.

ApiService has three externally meaningful publication states:

```text
Stale | Synchronizing | Live
```

Reset makes the old epoch Stale. The PostSeal trigger starts snapshot/replay
and publishes its coherent replacement as a new GraphEpoch in Synchronizing
state. Synchronizing is a valid current-DB image: head snapshots, deltas, and
SSE are available while processing catches up. The Live trigger changes that
same epoch to Live without another reload or epoch change. If Live arrives
while the replacement is still loading, remember the newer target and publish
the completed image directly as Live. A later Reset cancels any pending
publication and returns the API to Stale.

Each graph mutation and each active-epoch lifecycle-state change produces an
atomic revision. Reset retires its epoch and need not create an old-epoch
delta. A snapshot carries `(GraphEpoch, revision, publication_state)`. Deltas
describe a contiguous interval `(a,b)` within one epoch, are **independent of
requested window depth**, and compose sequentially:

```text
apply(Delta(a,b), image_at_a) = image_at_b
apply(Delta(b,c), image_at_b) = image_at_c
```

A lifecycle-only revision carries the publication-state update even when graph
data is unchanged.

Delta mutations are idempotent absolute set/upsert patches. They state the
resulting public block, block-state, level, publication-state, and coverage
values rather than relative operations such as increment, decrement, or
toggle. Reapplying one delta therefore produces the same graph state. Each
delta response uses its own response-local hash dictionary and carries its
target `HeadGraphCoverage`.

Gapless intervals in one GraphEpoch compose without reading the starting graph:

```text
compose(Delta(a,b), Delta(b,c)) = Delta(a,c)
```

Composition requires the same GraphEpoch and exact equality between the left
`to` and right `from` cursors. The result uses `a` as `from`, `c` as `to`, and
revision `c`'s coverage and publication state. A later absolute value for the
same entity wins; updates following an insertion fold into that block's final
public state; and reference-only endpoint metadata still required by a
crossing edge is retained. Composition decodes both response-local dictionaries
to hashes and constructs a self-contained dictionary for the result.

Composition is associative by graph-state effect. A composed encoding need not
be byte-identical to a delta constructed directly for the same interval, but:

```text
apply(compose(Delta(a,b), Delta(b,c)), image_at_a)
    = apply(Delta(b,c), apply(Delta(a,b), image_at_a))
```

A request from cursor `a` captures a desired target cursor `t`. If the complete
`Delta(a,t)` exceeds the response budget, return the largest nonempty prefix
`Delta(a,b)` that fits and ends at a complete revision boundary; the client
continues from `b`. Never split one block revision, atomic VSPC revision,
lifecycle-state revision, or its target coverage. If the first required atomic
revision cannot fit, incremental advancement is unavailable and the response
requires a fresh snapshot. Existing response-size and head-availability rules
apply if that snapshot cannot be served completely.

Delta retention is bounded. Epoch mismatch, unavailable revision, stale API,
or too-old cursor also requires a fresh snapshot. No outcome returns a
structurally partial revision. Whether a complete interval is encoded as
individual revision records or one coalesced absolute patch remains an
implementation choice under this composition contract.

SSE is only an ordered **cursor wakeup** `(epoch, revision)`, not the graph
data channel. A browser `EventSource` can receive a sequence on one
connection, but reconnection is not exactly-once. On connect the server
immediately emits the latest cursor; subsequent publications emit updated
cursors. Each client has a bounded cursor buffer. Slow clients get coalesced
cursor notifications and, if persistently behind, are disconnected; they
reconnect and use HTTP delta or snapshot.

`representation_version` is the settled term for the graph payload schema.
An ETag for a head snapshot distinguishes epoch, revision, effective window,
negotiated response format, `representation_version`, and publication state;
`Cache-Control: no-cache` allows cheap revalidation/304. A fixed delta
interval `(epoch,from,to)` is immutable and cacheable. A "to current" query
must revalidate. SSE has no ETag. Historical DB windows have **no ETag in
v2**, because computing an authoritative validator would itself require DB
work. SSE stays small text cursor events.

## Reset and recovery-time availability — settled

The existing reliable processing-to-ApiService control path carries
conceptual `Reset`, `PublishPostSeal`, `PublishLive`, and `InvalidateSession`
controls; no new recovery component is needed. The exact send points and
ordering belong to
[processing-lifecycle.md](processing-lifecycle.md#api-session-replacement-and-publication).
ApiService accepts exactly one Reset for each prepared processing session.
Common Reset effects are:

- mark the previously published HGC image Stale while allowing that coherent
  old image to remain readable;
- stop old-epoch deltas;
- prevent pending observer updates from the previous processing session from
  entering the replacement epoch; and
- arm buffering for the new session before processor Begin.

Reset has a completed-effect acknowledgement. The acknowledgement establishes
those effects but does not mean that a replacement image has been published.
Ordinary graph-update loss semantics do not weaken this reliable control
barrier.
PostSeal and Live are reliable, exact-once, state-specific controls, but they
are not processing barriers and have no publication-completion
acknowledgement.

`InvalidateSession` is a reliable, exact-once terminal control for a recoverably
aborted processing session. It marks that session's published image Stale,
stops its deltas, cancels any pending publication, and rejects later observer
updates from the invalidated session. It creates no GraphEpoch, performs no DB
reload, and does not arm buffering for a replacement session. The next
prepared session still begins with its own Reset. Invalidation leaves
historical reads available unless the invalidated session's database-rebuild
Reset had already closed them; in that case they remain closed until a later
PostSeal publication reopens them.

An ordinary Resync Reset leaves new and in-flight historical DB reads
available.

The database-rebuild Reset has the additional completed effect of closing new
historical DB reads and boundedly draining or cancelling existing ones before
processing data is cleared.
Historical window requests during `RebuildingDatabase` and `PreSeal` fail
cleanly with `503 Service Unavailable` and a short `Retry-After`. Merely
catching SQL errors is insufficient: a query against a partly reconstructed
DB can succeed but return an incomplete graph. A read already in flight may
return its old coherent snapshot if it completes before the reset barrier;
otherwise cancel it and return 503. API reads cannot indefinitely delay
processing.

PostSeal starts the consistent snapshot plus buffered-update replay. Once
coherent, ApiService publishes a new GraphEpoch in Synchronizing state; this
publication reopens historical reads after Rebuild. Processing does not wait
for it to complete.

The Live trigger makes the publication-state change atomically as a new
revision in the same GraphEpoch, so snapshot validators, deltas, and SSE
clients observe it without another reload. If PostSeal loading has not
finished, ApiService records the newer target and publishes the completed
image directly as Live.

If `TRUNCATE` is used inside the atomic rebuild, its transactional rollback
does **not** make it generally MVCC-safe for concurrent pre-existing
snapshots. Implementation must prove pre-reset reads return one old coherent
image or get canceled, never mixed old/new tables. See PostgreSQL's
[`TRUNCATE`](https://www.postgresql.org/docs/current/sql-truncate.html) and
[MVCC caveat](https://www.postgresql.org/docs/current/mvcc-caveats.html)
documentation. The API read barrier and bounded queries serve this contract;
the selected reset mechanism must preserve the same observable behavior. Its
detailed cancellation and transaction mechanism remains deferred in the
[decision register](../decisions/deferred.md).

## DAA navigation and graph windows — settled

```rust
struct GraphWindowResolution {
    resolved_level: u64,
    effective_start_level: u64,
    effective_end_level: u64,
}
```

Every successful anchored window response carries `GraphWindowResolution`,
independently of whether its one anchor is a level, block hash, or DAA score.
The resolved level is the fixed focus selected for that request, and the
effective bounds are the actual capped block-level range returned around it:

```text
effective_start_level <= resolved_level <= effective_end_level
```

For a level anchor, `resolved_level` is the retained requested level. For a
block-hash anchor, it is the materialized block's coordinate level. For a DAA
anchor, it is the VSPC-floor result defined below. Resolution and graph contents
come from the same immutable HGC image or consistent database transaction.

Accept `q` only within the shared
[`0..=MAX_DAA_SCORE` range](domain-model.md#shared-value-types--settled), then
resolve a DAA target by current VSPC floor, with the **highest level** among
score ties. The
[storage contract](storage.md#historical-read-contracts--settled) owns the
indexed database lookup and consistent historical transaction.

There may be VSPC-empty levels after reorg. If `q` precedes the retained PP
and no floor exists, report explicit no-retained-match. A `q` beyond the
current VSPC DAA resolves the current VSPC level. If `q` is at or above the
DAA of HGC's lowest cached VSPC level, HGC can resolve it; otherwise use the
storage lookup. Level resolution and its window must come from **one immutable
HGC image** or one consistent DB transaction, never different revisions.
ApiService derives cached levels' final scores from `VspcCommitted` and its
cached block metadata; storage sends no level-score delta. Historical
DAA/window responses are not cached in v2.

The public graph API has conceptually:

- head snapshot, depth-independent delta, and SSE cursor wakeup;
- one capped window operation with exactly one anchor: level, block hash, or
  DAA score; the anchor resolves once to a fixed level; and
- status/info covering network, processing/API versions, node state, and
  current validated node server version.

Exact endpoint URLs, HTTP methods, the final wire schema, and the graph wire
format remain deferred in the [decision register](../decisions/deferred.md).

Every graph response carries its hash dictionary. A window fully served by HGC
has a live cursor and `HeadGraphCoverage` in addition to its
`GraphWindowResolution`. A historical DB-backed window is a static image
without a cursor, capped by `MAX_WINDOW_DEPTH`. Requests crossing HGC's lower
bound take the consistent DB path; head depth itself never forces this
fallback.

## Resource isolation and saturation — settled

ApiService uses mandatory bulkheads beneath the system-wide processing
priority:

- a capped read-only API database pool separate from processing database
  capacity;
- bounded HTTP concurrency, query duration, response bytes and serialization
  CPU;
- bounded SSE clients and per-client buffers;
- bounded delta history, cache memory, and historical-read work;
- a separate memory-only status/info admission lane, so graph saturation
  cannot hide service state; and
- distinct budgets for head delivery and historical database reads.

API snapshot reload ranks above historical queries and below processing. On
saturation, reject or degrade API work explicitly. Do not block processing,
truncate a response or cache image presented as complete, or silently drop a
processing notification.

If all `MAX_CACHE_DEPTH = 1000` complete levels exceed the cache memory
allowance, first drop an optional stale image when useful. Otherwise mark head
temporarily unavailable and retry a complete reload. Never publish partial
levels. Slow SSE clients follow the bounded coalescing and disconnect contract
above.

V2 exposes these operational measurements:

- request count, latency, and response bytes by endpoint;
- active and rejected SSE clients;
- cache hits, misses, and evictions;
- database permit and query time;
- delta-journal resets and slow-client disconnects; and
- BlockProcessor and VspcProcessor commit latency.

The metrics export mechanism and labels remain deferred in the
[decision register](../decisions/deferred.md).

API traffic up to configured rejection limits must not materially increase
either processor's commit latency. Exact API capacities and resource budgets
remain deferred in the [decision register](../decisions/deferred.md).
