# API architecture

> Focused extraction; during the documentation reorganization, the current
> consolidated contract remains
> [handoff-2026-09-20.md](handoff-2026-09-20.md), which prevails on conflicts.

## Scope and ownership

This document owns ApiService, graph publication, the `HeadGraphCache`, and
the public graph API contract. The
[processing lifecycle](processing-lifecycle.md) owns when recovery milestones
send API controls. [Storage](storage.md) owns database transaction and query
implementation. Web client behavior belongs to the
[Web architecture](web.md).

## In-process API and graph observer feed — settled

KGI v2 includes an in-process `ApiService` and a complete bounded-by-level
`HeadGraphCache` (HGC), rather than directing each head request to expensive
PostgreSQL graph queries. This remains a valid foundation for future scale:
later deployments could separate multiple read-only API replicas from a
single writer/processor, but multiple full processing stacks and DB
replication are not required in v2.

Processors send `BlockCommitted`/`VspcCommitted` through **one ordered,
bounded graph-update channel**, after their respective DB commits. The
[BlockProcessor delivery contract](block-processing.md#committed-block-delivery)
and VspcProcessor's corresponding producer contract establish causal order,
so a VSPC update cannot reach this channel ahead of blocks it depends on.

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

The parent payload includes both direct and selected-parent coordinates
without duplicating the selected parent. Updates for blocks outside current
HGC still need examination: a new block can increase `levels.size` at an
external parent endpoint used by an edge crossing the cache boundary. The
size of an external level may be seeded by an incoming child's parent data
and then updated monotonically when later blocks occupy it.

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
`MAX_WINDOW_DEPTH <= MAX_CACHE_DEPTH`; its exact value remains to select.
No arbitrary maximum block count may truncate a retained level: **every**
block and relevant edge endpoint for each cached level is available. Every
windowed endpoint caps requested depth to `MAX_WINDOW_DEPTH` and reports
the effective range. An oversized `/graph/head` request cannot fall back to
DB; it is capped. A cache memory budget may make the whole image temporarily
unavailable, never partially populated. Process work retains resource
priority.

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
drawable edge. This representation allows Genesis recognition without a
persisted network Genesis hash or a dedicated Genesis-hash API endpoint.

## Snapshot, revision, delta, SSE, and ETags — settled

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

Absolute set/upsert patches are preferable to fragile relative instructions.
Each delta response uses its own response-local hash dictionary. Retention is
bounded. Epoch mismatch, unavailable revision, stale API, or too-old cursor
requires a fresh snapshot, never a partial delta. Whether adjacent revisions
are encoded as individual records or a coalesced patch is implementation
choice as long as the composability contract holds.

SSE is only an ordered **cursor wakeup** `(epoch, revision)`, not the graph
data channel. A browser `EventSource` can receive a sequence on one
connection, but reconnection is not exactly-once. On connect the server
immediately emits the latest cursor; subsequent publications emit updated
cursors. Slow clients get coalesced cursor notifications and, if persistently
behind, are disconnected; they reconnect and use HTTP delta or snapshot.

`representation_version` is the settled term for the graph payload schema.
An ETag for a head snapshot distinguishes epoch, revision, effective window,
negotiated response format, `representation_version`, and publication state;
`Cache-Control: no-cache` allows cheap revalidation/304. A fixed delta
interval `(epoch,from,to)` is immutable and cacheable. A "to current" query
must revalidate. SSE has no ETag. Historical DB windows have **no ETag in
v2**, because computing an authoritative validator would itself require DB
work. Exact graph wire format is intentionally not chosen yet: benchmark
JSON versus appropriate binary formats (CBOR, MessagePack, Protobuf, etc.)
over server construction/serialization, compression, transfer, browser
decode, and graph-model construction. SSE stays small text cursor events.

## Reset and recovery-time availability — settled

The existing reliable processing-to-ApiService control path carries
conceptual `Reset`, `PublishPostSeal`, and `PublishLive` controls; no new
recovery component is needed. Every prepared processing session sends exactly
one Reset. Common Reset effects are:

- mark the previously published HGC image Stale while allowing that coherent
  old image to remain readable;
- stop old-epoch deltas;
- prevent pending observer updates from the previous processing session from
  entering the replacement epoch; and
- arm buffering for the new session before processor Begin.

Reset has a completed-effect acknowledgement. The acknowledgement establishes
those effects but does not mean that a replacement image has been published.
Ordinary graph-update loss semantics do not weaken this reliable control
barrier. The exact command representation remains an implementation detail.
PostSeal and Live are reliable, exact-once, state-specific controls, but they
are not processing barriers and have no publication-completion
acknowledgement.

The processing lifecycle owns the send points and ordering of these controls.
For ordinary Resync, read-only reconciliation finishes before Reset. A failed
reconciliation requests a separate Rebuild without resetting ApiService. A
successful Resync Reset leaves new and in-flight historical DB reads
available.

For Rebuild, the database-rebuild Reset occurs before processing data is
cleared. This Reset has the additional completed effect of closing new
historical DB reads and boundedly draining or cancelling existing ones.
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
implementation may add safe transaction locking or choose another reset
strategy without changing observable behavior.

## DAA navigation and graph windows — settled

For valid query `0 <= q < i64::MAX`, resolve a DAA target by current VSPC
floor, with the **highest level** among score ties. The
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
DAA/window response caching is excluded from v2 and recorded for v2.1.

The public graph API has conceptually:

- head snapshot, depth-independent delta, and SSE cursor wakeup;
- one capped window operation with exactly one anchor: level, block hash, or
  DAA score; the anchor resolves once to a fixed level; and
- status/info covering network, processing/API versions, node state, and
  current validated node server version.

Exact paths, methods, and final wire schema are implementation/API design
details. There is no `/blockHashesByIds`-style endpoint in v2 because every
graph response carries its hash dictionary. A window fully served by HGC has
a live cursor. A historical DB-backed window is a static image without a
cursor, capped by `MAX_WINDOW_DEPTH`. Requests crossing HGC's lower bound
take the consistent DB path; head depth itself never forces this fallback.
