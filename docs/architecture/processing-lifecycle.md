# Processing lifecycle and recovery

> Focused extraction; the current consolidated contract is
> [handoff-2026-09-20.md](handoff-2026-09-20.md), which prevails on conflicts.

## Processing session and resource acquisition — settled

```rust
struct ProcessingSession {
    rpc: Arc<ValidatedRpcClient>,
    db: Arc<ValidatedDbClient>,
}
```

Supervisor waits for usable node and storage generations only when a recovery
is desired and the engine is idle. The two waits are polled concurrently with
Supervisor commands/events; the Supervisor main loop must not block.

Partial acquisition may be retained while waiting for the other resource.
Before starting, recheck the latest desired recovery and that the engine is
still idle.

The exact `Arc<ValidatedRpcClient>` and `Arc<ValidatedDbClient>` are
passed in `Start` and then through the session to all consumers, including
DependencyResolver. This guarantees one resource generation per run.

## Supervisor and recovery intent — settled

There is no `Auto` recovery mode.

```rust
enum RecoveryMode {
    Resync,
    Rebuild,
}
```

Implement `Ord` with:

```text
Resync < Rebuild
```

Document in code that this ordering represents **recovery strength**, not
lifecycle or execution order.

Startup behavior:

- normal startup requests `Resync`;
- `--clear-db` requests `Rebuild`.

`desired_recovery` is the strongest recovery requirement the Supervisor must
eventually satisfy. Sending `Start` does not consume it. `active_recovery` is
the recovery currently executing.

Repeated requirements coalesce with `max`. If a stronger requirement arrives
during an active run, request deactivation once, wait for `Idle`, then start
the stronger mode.

```rust
enum FaultDisposition {
    Retry,
    Require(RecoveryMode),
    Fatal,
}
```

- `Retry` is meaningful only while a recovery is active.
- A recoverable fault in Live must require at least `Resync`.
- Fault strings are diagnostics and must never drive control flow.

Fault ownership:

```text
DependencyResolver / OrphanManager -> BlockProcessor
BlockProcessor / VspcProcessor     -> ResyncEngine
ResyncEngine                       -> Supervisor
```

`Rebuild` intent must not outlive successful PP-boundary sealing. At the
`PpBoundarySealed` milestone, Supervisor downgrades both desired and active
recovery to `Resync`. Entering Live satisfies and clears the remaining recovery
requirement.

Administrative/API-triggered recovery through this same Supervisor path is a
KGI v2.1 candidate, not a v2 requirement.

## ResyncEngine — settled

Commands:

```text
Start { mode, rpc, storage }
Deactivate
Shutdown
```

`Deactivate` replaces the earlier name `Quiesce` with the same barrier
semantics.

The engine prepares one common structure for Resync and Rebuild:

```rust
struct PreparedSync {
    rpc: Arc<ValidatedRpcClient>,
    db: Arc<ValidatedDbClient>,
    anchor: MaterializedSyncAnchor,
    boundary_seal_blue_score: u64,
}
```

Do not add a redundant `ProcessingResources` wrapper.

Once prepared, both modes use the same block/VSPC synchronization pump. Their
only material difference is storage policy before PP-boundary sealing.

### Resync preparation

Storage reconciliation obtains:

- database PP at `(level = 1, slot = 0)`;
- `db_pp_blue_score` from metadata;
- committed materialized VSPC sink derived as the maximum-ID materialized
  block with `is_in_vspc = true`.

Empty state is allowed only when fully empty. Partial state is inconsistent.

Resync requirements:

1. The current node PP is boundary-materialized in the database.
2. The node recognizes the committed materialized VSPC sink used as `low_hash`.
3. `sink.blue_score >= db_boundary_seal_blue_score`, where:

   ```text
   db_boundary_seal_blue_score =
       db_pp_blue_score + anticone_finalization_depth
   ```

Failure to reconcile reports `Require(Rebuild)` to Supervisor. Rebuild occurs
as a separate run; there is no internal Auto fallback.

### Rebuild preparation

The only storage API that clears processing data is:

```rust
rebuild_from_pruning_point(pp: SharedNodeBlock)
    -> MaterializedSyncAnchor
```

The pruning point is mandatory. The operation atomically clears processing
data, restarts identity allocation, creates required PP-boundary identities,
materializes PP at `(1, 0)`, marks it in VSPC, initializes `levels`, stores
`db_pp_blue_score`, resets/seeds caches after commit, and returns the anchor.

Configuration, schema, and useful node metadata survive the rebuild.

### Deactivation barrier

Conceptual order:

1. close local routing gates and disable notifications;
2. cancel and join synchronization producers;
3. clear engine-local buffers;
4. send `Deactivate` to processors;
5. await processor acknowledgements, including descendants;
6. release all processing-session clones of validated RPC/storage handles;
7. drop `ProcessingSession`;
8. emit `Deactivated` / enter `Idle`.

On application shutdown: stop engine/processors first, then NodeService, then
StorageService.


## Resync block/VSPC pump — settled

The common pump holds a normalized GetBlocks page and one pending VSPC V2
response.

```text
request VSPC V2 from current low_hash
determine its destination
dispatch GetBlocks blocks until that destination has been sent to BlockProcessor
pause the remaining page suffix
send the synthetic VspcChange to VspcProcessor
request the next VSPC response
resume the page suffix
```

"Sent" means accepted by the BlockProcessor channel, not committed. Processor
failures either directly request recovery or cascade into an unprocessable
VSPC backlog that requests recovery.

Notifications go directly to processors; the engine never journals or
redispatches them.

### Entering recovery phases

Before a fresh Begin:

1. disable/unsubscribe processing notifications;
2. send Begin to BlockProcessor and VspcProcessor.

Begin needs no acknowledgement. Before Catchup:

1. send Catchup commands, including lower bounds;
2. then enable/subscribe processing notifications.

Processor-local notification gates are authoritative for immediate dropping.

### Catchup trigger

Outside the unordered sink anticone, normalized GetBlocks blocks are strictly
monotonic by `ConsensusOrder`. Enter Catchup immediately before dispatching the
first response that satisfies either:

- a nonmonotonic block appears in the normalized page; or
- the normalized response is underfilled, defined as fewer than three blocks.

An empty VSPC V2 page is a fallback Catchup trigger if block pages did not
trigger first. Requery VSPC at the target block interval (`bps`-derived) until
a nonempty response arrives. This wait gives block processing and notification
delivery time without adding a second elaborate sink-anticone estimator.

The previously explored `unordered_sink_capacity`, tenfold mergeset margin,
half-window subscription point, X-calls/Y-blocks rules, and explicit epochs are
rejected as unnecessary complexity.

### Late notification filtering without epochs

There is no notification epoch because late messages after unsubscribe cannot
be reliably distinguished from early messages after resubscribe.

Begin resets all processor-local state and drops notifications until Catchup.
Catchup supplies objective lower bounds:

- BlockProcessor receives a CompactId lower bound associated with the latest
  already-materialized GetBlocks anchor/page when Catchup was triggered. A
  known notification below the bound is discarded without overlap credit.
  Unknown blocks are not discarded by guessed order. Notifications below the
  sealed PP are also invalid/discardable.
- VspcProcessor receives the synthetic sink lower bound that triggered
  Catchup. Older notification destinations are discarded without overlap
  credit.

This replaces a timer-based grace period. Legitimate late BlockAdded messages
are otherwise harmless: they deduplicate, orphan, or persist normally.

## Catchup overlap and transition to Live — settled

Each processor owns an `AtomicBool` overlap flag shared read-only with the
engine. Begin resets it. Catchup begins measurement.

### Blocks

Track seen hashes with one map and source bitmask, not two sets:

```text
SYNTHETIC = 0b01
NOTIFICATION = 0b10
```

There are no duplicates within the normalized resync stream, and ordinary
BlockAdded delivery is expected not to duplicate within one subscription.
Seeing both bits for one hash proves overlap between the sources and sets the
block overlap flag.

Evaluate the flag only at a fully dispatched GetBlocks-page boundary. Once
overlap is proven, no buffered suffix from an undispatched page remains; a page
already returned by GetBlocks must be fully dispatched before it can be
dropped from engine state.

Orphans do not prevent transition to Live.

### VSPC

During Catchup, synthetic changes are the exclusive commit source. They are
gapless and ordered and are held in a queue. Notifications are still consumed,
resolved, filtered, and retained as needed to establish overlap, but do not
overtake synthetic commits.

Notifications below the Catchup synthetic-sink lower bound are latecomers and
are discarded without overlap credit. A notification transition can cross the
committed synthetic sink; equality of its destination is not required. The
accepted overlap logic must therefore use source/destination continuity rather
than the disproved claim that a source below the synthetic sink implies an
equal destination.

Once notification history provably meets/crosses the committed synthetic
history, the VSPC overlap `AtomicBool` is set. Gapless notification flow then
guarantees eventual reachability of the synthetic point.

### Live condition

At a fully dispatched GetBlocks-page boundary:

```text
block_overlap == true
&& vspc_overlap == true
```

is sufficient to enter Live. Do not require an empty orphan store or inspect
additional VspcProcessor pending state.

On Live:

- stop the synthetic pump;
- BlockProcessor retains valid queued/orphan work;
- VspcProcessor discards its uncommitted synthetic queue and begins committing
  the notification flow.

Any latent inconsistency will be detected by the normal Live invariants and
will request recovery.

## Short node IBD while Live — settled stance

The current design is considered sufficiently resistant to a rare short IBD
episode after KGI is already Live: connection/notification failures trigger
recovery, while normal invariants reject inconsistent progress. Do not add a
complex continuous IBD mode for v2.

A low-frequency Live VSPC consistency probe is recorded as a KGI v2.1
candidate.
