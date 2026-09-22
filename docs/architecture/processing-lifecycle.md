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
Before starting, recheck the latest desired recovery, engine Idle, and that
both services still publish those exact acquired RPC and DB `Arc` generations
as Ready.

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

An Empty network-bound DB has no PP or sink for Resync reconciliation. The
engine reports `Require(Rebuild)`; Supervisor then starts a distinct
Rebuild run. Structurally valid but inconsistent processing contents also
require Rebuild, while partial/unsupported schema is rejected by
StorageService before publishing a DB client.

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
- A nonmaterialized hash directly named in VSPC `added`/`removed` and a
  resolver-confirmed unavailable dependency each require Rebuild directly:
  the DB can no longer be trusted against node state. RPC connection failure
  is not proof of dependency unavailability.

Fault ownership:

```text
DependencyResolver / OrphanManager -> BlockProcessor
BlockProcessor / VspcProcessor     -> ResyncEngine
ResyncEngine                       -> Supervisor
```

Faults and milestones use reliable owner-directed events; latest-value
status may skip intermediate states. Each run retains its first causal fault
diagnostically. Unexpected permanent-worker exit, panic, closed command
channel, or invalid forward command is a typed ownership/session or fatal
fault, not ordinary DAG discontinuity. A dropped barrier acknowledgement
receiver does not cancel the worker's completed teardown transition.

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

An Empty state is genuinely fully empty and requests a distinct Rebuild run
because PP, score, and sink are absent. A valid schema with inconsistent
processing contents also requests Rebuild but is not treated as Empty.

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
When the PP's selected parent is ORIGIN, the transaction creates that
outside-boundary identity, keeps `selected_parent_id` non-null, and does
not invent a Genesis direct parent.

Database network binding and schema/migration state survive the rebuild. The
transaction replaces processing data and PP-derived processing metadata,
including `db_pp_blue_score`.

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
Request VSPC V2 with `min_confirmation_count = None` and
`data_verbosity_level = Some(RpcDataVerbosityLevel::None)`. This combination
was verified against rusty-kaspa master `c338d495`: explicit `None` verbosity
retains a minimal acceptance-data envelope and a nonempty response has an
advancing `added.last()` cursor. The acceptance-data budget may shorten the
response to a complete prefix. Preserve this behavior with a pinned regression
fixture. An empty V2 page is a pump/Catchup hint, not a VSPC change.

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
2. start both remote subscriptions with NotificationRouter still Disabled;
3. after both starts succeed, enable the router and publish the client's
   subscription state Enabled.

Processor-local notification gates are authoritative for immediate dropping.
Callbacks arriving while the router remains Disabled during activation are
intentionally dropped without overlap credit or a recovery request. Synthetic
pumps continue until ordinary overlap is demonstrated. Before Live, the fixed
body-tip coverage gate below accounts for activation-time drops, including a
block outside the selected past; there is no separate callback replay.

### Catchup trigger

Capture `catchup_sink = GetSink()` and the DAA score of that exact block at
the start of the GetBlocks scan. Track that rolling marker through the
ordered synthetic VSPC pump:

```text
Unknown -- marker equals the synthetic cursor or occurs in added --> Present
Present -- marker in a later removed --> Removed
```

Only `Present` can authorize Catchup. Cursor equality covers an already
synchronized marker that will not be repeated in `added`. `Removed`
invalidates the marker. An `Unknown` marker's absence from `removed` is not
evidence because it may have been reorged before the pump admitted it. The
VSPC RPC's removed suffix is complete even when its added path is batch
limited.

Hold a normalized GetBlocks page containing the marker before dispatch and
refresh `GetSink()`. Fetch the returned block's immutable header and use
checked DAA-score subtraction. The accepted proximity threshold is:

```text
catchup_max_daa_gap = max(
    30 * network_bps,
    mergeset_size_limit + 1,
)
```

The second term provides at least one complete GetBlocks core page of
transition granularity. The values are 181 at 1 BPS, 300 at 10 BPS, and 960
at 32 BPS; all are below the VSPC V2 batch size.

If the marker is `Present`, the fresh score is not lower, and the gap is at
most the threshold, queue both Catchup commands and complete subscription
activation before dispatching the entire held page under Catchup. If the gap
is larger, replace the marker with the fresh sink, reset it to `Unknown`, and
remain in Resync. Re-evaluate immediately when the replacement is already in
the held page. Otherwise continue scanning. Remember a marker already seen by
GetBlocks when its VSPC state has not caught up, and reconsider eligibility at
a later complete page boundary. A removed marker or decreasing score is
replaced and cannot authorize Catchup.

The rolling marker is the primary path. If it has not authorized Catchup,
retain three independent fallbacks:

```text
the normalized GetBlocks page's global maximum ConsensusOrder is not final
OR
the normalized GetBlocks page contains fewer than three blocks
OR
the VSPC V2 page is empty
```

A stable sink-reaching page with appended anticone leaves the sink maximum
before the final block. Under the `L + 1` GetBlocks core budget, a below-sink
capacity stop has at least three normalized blocks, so `< 3` is a one-way
sink-reaching proof. Evaluate these predicates on the complete page and enter
Catchup before dispatching it.

An empty VSPC V2 page enters Catchup without moving its cursor. Requery from
the same cursor at the target block interval; a short nonempty page is not a
fallback. The rolling marker is the normal path and these fallbacks cover an
exceptional failure to transition through it. A material omission exposed
under strict pre-Catchup processing uses the existing `Require(Resync)` fault
path; there is no separate mixed-view recovery protocol. The synthetic pump
continues through Catchup and observes later reorgs. Rebuild must have
definitely committed and emitted
`PpBoundarySealed` before the primary path or a fallback can enter Catchup.
Eligibility while still PreSeal fails the current recovery.
Resync starts PostSeal only after reconciliation.

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

One normalized GetBlocks response has unique hashes, but the same hash may
appear in successive responses, especially in a repeated sink anticone.
ResyncEngine owns an exact Catchup-only `catchup_sent` set, inserts only after
successful synthetic-channel enqueue, and filters later synthetic repeats
without overlap credit. Ordinary BlockAdded delivery is expected not to
duplicate within one subscription. Seeing both source bits for one hash proves
overlap and sets the block overlap flag.

Evaluate the flag only at a fully dispatched GetBlocks-page boundary. Once
overlap is proven, no buffered suffix from an undispatched page remains; a page
already returned by GetBlocks must be fully dispatched before it can be
dropped from engine state.

Orphans do not prevent transition to Live.

### VSPC

While the coordinated Catchup pump supplies synthetic changes, those gapless
ordered changes have commit priority. Notifications are still consumed,
resolved, filtered, and retained as needed to establish overlap, but do not
overtake an available synthetic predecessor. Once ResyncEngine stops producing
synthetic changes, ordinary readiness processing can commit retained/new
actionable notifications without a coverage-phase signal.

Notifications below the Catchup synthetic-sink lower bound are latecomers and
are discarded without overlap credit. A notification transition can cross the
committed synthetic sink; equality of its destination is not required. The
accepted overlap logic first discards a resolved destination at or below the
committed sink under the lower-bound/history rules. Destination equality can
earn eligible overlap credit but never creates an empty normalized change.
For a destination above the sink, structural crossing requires the exact
committed sink in `added`; drop `removed` and the prefix through that sink,
leaving a necessarily nonempty added-only suffix. Otherwise only an original
source equal to the committed sink is directly actionable. Order comparison
or sink occurrence in `removed` does not prove a crossing, and a pending
candidate must not block a later actionable one. An unresolved old
destination cannot earn overlap credit.

Once notification history provably meets/crosses the committed synthetic
history, the VSPC overlap `AtomicBool` is set. Gapless notification flow then
guarantees eventual reachability of the synthetic point.

### Ordinary eligibility and coverage admission

At a fully dispatched GetBlocks-page boundary:

```text
PostSeal == true
&& block_overlap == true
&& vspc_overlap == true
```

establishes ordinary Live eligibility. It does not yet authorize Live.

Stop issuing VSPC V2 calls and sending new synthetic changes. VspcProcessor is
not informed of the coverage phase and receives no command or barrier. It
continues its normal ordered readiness and commit processing over synthetic
changes already accepted and retained/new notification changes. Available
block material permits sink progress; unavailable material keeps the affected
change in pre-resolution readiness waiting. A chain member confirmed
nonmaterialized for an actionable transition remains a direct Rebuild fault.

Capture a fixed `GetBlockDagInfo.tip_hashes` set `T` after subscriptions are
Enabled and determine in one batch its already-materialized subset `M`. Do not
refresh `T`. The pinned upstream ordering commits each body-tip-store update
before emitting its BlockAdded notification, so blocks committed after the
snapshot are protected by the active subscription.

Run at least one additional block-only GetBlocks request using the block scan's
existing cursor. Preserve full-page dispatch and the Catchup-only dedup set.
Request starts are separated by at least one target block interval:

```text
coverage_page_interval = 1 second / network_bps
```

With merge-set limit `L`, cap additional fully dispatched responses at:

```text
1 + ceil(catchup_max_daa_gap / (L + 1))
```

The resulting current caps are 2, 3, and 3 pages at 1, 10, and 32 BPS, using
the pinned rusty-kaspa merge-set limits 180, 248, and 512. Empty and fully
filtered responses consume budget. The cap is operational and does not claim
a mathematical DAA advance per page.

After each complete page, Live admission requires:

```text
T ⊆ M ∪ catchup_sent
```

This establishes gapless admission: every retained pre-snapshot body tip is
materialized or has entered BlockProcessor, so missing ancestry will become
explicit queued/orphan dependency work. It does not require those blocks or
orphans to finish before Live. If the capped page completes without satisfying
the invariant, request `Require(Resync)`. The next recovery attempt derives its
starting sink from current committed database state.

On Live:

- stop and join the block coverage producer before sending both Live commands;
- BlockProcessor retains valid queued/orphan work;
- VspcProcessor continues from its actual committed sink.

Any latent inconsistency will be detected by the normal Live invariants and
will request recovery.
`EnteredLive` is emitted only after the block coverage producer is
stopped/joined and both Live commands are successfully enqueued. It does not
mean queues or orphan state are empty.

## Short node IBD while Live — settled stance

The current design is considered sufficiently resistant to a rare short IBD
episode after KGI is already Live: connection/notification failures trigger
recovery, while normal invariants reject inconsistent progress. Do not add a
complex continuous IBD mode for v2.

A low-frequency Live VSPC consistency probe is recorded as a KGI v2.1
candidate.
