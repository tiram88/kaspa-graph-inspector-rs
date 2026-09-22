# Processing lifecycle and recovery

> Focused extraction; the current consolidated contract is
> [handoff-2026-09-20.md](handoff-2026-09-20.md), which prevails on conflicts.

## Scope and ownership

This document owns Supervisor recovery intent, cross-worker commands and
faults, ResyncEngine preparation and pumping, recovery phase transitions,
Catchup admission, block coverage before Live, and teardown order.

It coordinates component behavior without redefining it. Node connection,
subscription, and normalization rules belong to
[node-service.md](node-service.md); persistence and transaction mechanics to
[storage.md](storage.md); processor-local admission and overlap rules to
[block-processing.md](block-processing.md) and
[vspc-processing.md](vspc-processing.md); and API publication effects to
[api.md](api.md). Shared value types belong to
[domain-model.md](domain-model.md).

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
Before starting, recheck that the captured recovery obligation is still the
latest `desired_recovery`, the engine is Idle, and both services still publish
those exact acquired RPC and DB `Arc` generations as Ready.

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

enum Component {
    Supervisor,
    NodeService,
    StorageService,
    ResyncEngine,
    BlockProcessor,
    OrphanManager,
    DependencyResolver,
    VspcProcessor,
    ApiService,
}
enum ServiceKind { Node, Storage }
enum NotificationStream { BlockAdded, Vspc, Both }
enum RecoveryInputKind {
    RemovedOnlySyntheticVspc,
    MalformedGetBlocks,
    MalformedVspcResponse,
}
enum PersistenceFault {
    DefiniteFailure,
    RetryExhausted,
    AmbiguousCommit,
}
enum FaultKind {
    ServiceGenerationLost(ServiceKind),
    NotificationContinuityLost(NotificationStream),
    RecoveryInputInvalid(RecoveryInputKind),
    ReconciliationFailed,
    MaterialityViolation,
    DependencyUnavailable,
    Persistence(PersistenceFault),
    Ownership,
}
struct ComponentFault {
    source: Component,
    disposition: FaultDisposition,
    kind: FaultKind,
    diagnostic: Arc<str>,
}

struct SupervisorStatus {
    lifecycle: SupervisorLifecycle,
    desired_recovery: Option<RecoveryMode>,
    active_recovery: Option<RecoveryMode>,
}
```

`deactivation_requested` is internal and absent from public status.
`SystemStatus` combines Supervisor, NodeService, StorageService, and processing
observations; it is eventually consistent and never a synchronization or
recovery input.

`ComponentFault` is the cross-worker control envelope. Component-local errors
may retain richer library-specific sources, but must be classified before
crossing an ownership boundary. The enum sketch fixes the required semantic
discriminants, not exact module placement or error-library syntax. Only typed
fields drive control, retry counters, and metrics; diagnostic strings never
do.

- `Retry` is meaningful only while a recovery is active. It aborts and fully
  deactivates the current attempt; after Idle and backoff, Supervisor reruns
  fresh preparation for the strongest unsatisfied recovery obligation. It
  never retries one RPC or page in place.
- A recoverable fault in Live must require at least `Resync`.
- Fault strings are diagnostics and must never drive control flow; retry
  causes used by policy are typed.
- A nonmaterialized hash directly named in VSPC `added`/`removed` and a
  resolver-confirmed unavailable dependency each require Rebuild directly:
  the DB can no longer be trusted against node state. RPC connection failure
  is not proof of dependency unavailability.

When the same validated RPC and DB generations remain Ready, whole-attempt
recovery retries use nominal delays `1s, 2s, 4s, 8s, 16s, 30s`, capped at
`30s`, with equal jitter from 50% through 100%. Reset that general backoff on
`EnteredLive`, a new validated RPC or DB generation, or a stronger recovery
obligation. Generation loss waits for the corresponding service reconnect
loop without adding this delay. `Require(Resync)` and `Require(Rebuild)` do not
consume or wait on the Retry sequence. All waits are lifecycle-cancellable.

For the typed synthetic removed-only VSPC fault, Supervisor retains a counter
across attempts on the same `ValidatedRpcClient` generation. The first three
occurrences each produce `Retry`; the fourth is `Fatal`. A new validated RPC
generation or `EnteredLive` resets it; changing recovery mode on the same
generation does not. Removed-only notifications require Resync and do not
consume this pump-specific budget.
The three permitted retries use the first three general recovery delay slots:
nominally `1s`, `2s`, and `4s`, with the same equal jitter.

Storage owns local transaction retries and ambiguous-outcome handling; see
[storage.md](storage.md#transaction-retries). Once classified across the
ownership boundary, `Persistence(RetryExhausted)` aborts active recovery with
`Retry` and requests `Require(Resync)` in Live. An ambiguous commit is never
blindly reissued.

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

Command mailboxes are unbounded and prioritized, with one logical producer per
worker. They do not impose data-channel backpressure. Data, notification, and
worker-to-worker channels are bounded and cancellation-aware. Full means
`Require(Resync)` when ordered session continuity may have been lost;
closed/unavailable means a session ownership fault; cancellation during
expected teardown is not a fault. The graph observer feed is the exception:
its loss invalidates the API image without disrupting processing.

`Start`, processor `Begin`, `Catchup`, and each processor's `Live` are
exact-once, state-specific commands. Duplicate or invalid-state delivery is
Fatal. The two `Live` sends need not be simultaneous; enqueue is not
completion. Only `Deactivate` and `Shutdown` have completed-barrier
acknowledgements. `Deactivate` is idempotent to Idle. `Shutdown` is terminal
and idempotent from every state, supersedes an in-progress Deactivate, and
acknowledges only after full shutdown. Unexpected command-channel closure is
Fatal while its worker is meant to live.

`Rebuild` intent must not outlive successful PP-boundary sealing.
`PpBoundarySealed` is an exact-once upward milestone event, never a command to
BlockProcessor. BlockProcessor emits it under its
[PP-boundary contract](block-processing.md#pp-boundary-phase-behavior--settled).
ResyncEngine observes it before permitting Catchup, sends ApiService the
PostSeal publication trigger, and propagates the milestone to Supervisor.
Supervisor then downgrades both desired and active recovery to `Resync`.
Duplicate or invalid-state milestone delivery is Fatal. Entering Live satisfies
and clears the remaining recovery requirement.

Administrative/API-triggered recovery through this same Supervisor path is a
KGI v2.1 candidate, not a v2 requirement.

## ResyncEngine — settled

Commands:

```text
Start { mode, rpc, db }
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
  block with `is_in_vspc = true`, including its ID, hash, and stored DAA
  score.

The storage query and committed-sink derivation are defined in
[storage.md](storage.md#historical-read-contracts--settled). ResyncEngine owns
the orchestration and node-side validation below.

An Empty state is genuinely fully empty and requests a distinct Rebuild run
because PP, score, and sink are absent. A valid schema with inconsistent
processing contents also requests Rebuild but is not treated as Empty.

ResyncEngine uses the run's exact `Arc<ValidatedRpcClient>` to call
`GetBlock(sink_hash, include_transactions = false)`. It requires the returned
header hash to equal the requested sink hash and its DAA score to equal the
stored sink DAA score. It then constructs `MaterializedSyncAnchor` from the
stored ID and hash plus the header's blue work and blue score. A header-only
node block is sufficient because no body or transactions are needed. At the
pinned rusty-kaspa revision, successful GetBlock GhostDAG enrichment also
establishes the recognition required to use the sink as a GetBlocks
`low_hash`.

Resync requirements:

1. The current node PP is boundary-materialized in the database.
2. The node recognizes the committed materialized VSPC sink used as `low_hash`.
3. `sink.blue_score >= db_boundary_seal_blue_score`, where:

   ```text
   db_boundary_seal_blue_score =
       db_pp_blue_score + anticone_finalization_depth
   ```

A missing or inconsistent stored sink, a definitive absent/invalid response
from the node, a stored/returned DAA-score mismatch, or another failed
reconciliation check reports `Require(Rebuild)` to Supervisor. A transport
failure, cancellation, connection loss, or validated-client loss is instead a
session fault/retry and does not prove that Rebuild is required. A returned
hash other than the requested sink is a malformed RPC response and a
protocol/session fault. Rebuild occurs as a separate run; there is no internal
Auto fallback.

The special recovery behavior for an initialized database whose retained
pruning point is Genesis remains an open requirement; see
[open-questions.md](../open-questions.md). This section does not infer a policy
beyond the settled Genesis/ORIGIN representation.

### Rebuild preparation

ResyncEngine obtains the mandatory current pruning-point block from the run's
validated RPC generation, then calls the only storage API that clears
processing data:

```rust
rebuild_from_pruning_point(pp: SharedNodeBlock)
    -> MaterializedSyncAnchor
```

The pruning point is mandatory. Storage owns the transaction, retained network
binding, PP-boundary representation, metadata replacement, cache publication,
and returned anchor; see
[storage.md](storage.md#rebuild-transaction--settled). ResyncEngine must not
expose or use a general clear-without-PP primitive.

### API session replacement and publication

Every prepared processing run replaces API publication continuity through the
existing reliable processing-to-ApiService control path. The lifecycle sends
these controls in session order:

```text
Reset -> PublishPostSeal -> PublishLive
```

The effects of these controls, including historical-read availability and
GraphEpoch behavior, are defined in
[api.md](api.md#reset-and-recovery-time-availability--settled). This document
owns their send points. `Reset` has a completed-effect acknowledgement;
`PublishPostSeal` and `PublishLive` are reliable and exact-once, but processing
does not wait for publication completion.

For ordinary Resync, perform read-only reconciliation first. A failed
reconciliation requests Rebuild without resetting the API. After successful
reconciliation, send the session-replacement Reset and await its
acknowledgement, then send the PostSeal publication trigger before processor
Begin.

For Rebuild, obtain the pruning point, send the database-rebuild Reset, and
await its acknowledgement before `rebuild_from_pruning_point`. After processor
Begin, ResyncEngine sends the PostSeal publication trigger only when it
observes BlockProcessor's definitely committed `PpBoundarySealed` event.

When the global `EnteredLive` conditions are satisfied, ResyncEngine sends the
Live publication trigger.

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
A response with empty `added` and nonempty `removed` violates the pinned sink
monotonicity invariant. Do not dispatch it or advance the cursor; abort the
complete recovery attempt using the bounded synthetic removed-only `Retry`
policy above.

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

Each incremental VSPC request uses the preceding response destination as its
next `low_hash`. This makes the synthetic VSPC stream ordered and gapless.

Notifications go directly to processors; the engine never journals or
redispatches them.

### Entering recovery phases

Before a fresh Begin:

1. disable/unsubscribe processing notifications;
2. complete the API Reset acknowledgement; for Resync also send
   `PublishPostSeal`, while Rebuild waits for the post-Begin seal event;
3. send Begin to BlockProcessor and VspcProcessor.

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
continues through Catchup and observes later reorgs. ResyncEngine must have
observed BlockProcessor's definitely committed `PpBoundarySealed` event before
the primary path or a fallback can enter Catchup. Eligibility while still
PreSeal fails the current recovery.
Resync starts PostSeal only after reconciliation.

### Late notification filtering without epochs

There is no notification epoch because late messages after unsubscribe cannot
be reliably distinguished from early messages after resubscribe.

Begin resets processor-local state and closes notification gates. Catchup
supplies the objective block and VSPC lower bounds derived from the current
session anchors. Their exact filtering and credit rules belong to
[block-processing.md](block-processing.md#catchup-filtering-and-overlap) and
[vspc-processing.md](vspc-processing.md#catchup-filtering-crossing-and-overlap--settled).
The engine does not add a timer-based grace period or transport epoch.

## Catchup overlap and transition to Live — settled

Each processor owns an `AtomicBool` overlap flag shared read-only with the
engine. Begin resets it and Catchup begins measurement. ResyncEngine reads
both flags only at a fully dispatched GetBlocks-page boundary.

### Blocks

BlockProcessor owns source accounting, late filtering, overlap proof, and the
rule that valid orphans do not prevent Live; see
[block-processing.md](block-processing.md#catchup-filtering-and-overlap).

ResyncEngine owns an exact Catchup-only `catchup_sent` set. Initialize it on
Catchup, insert a hash only after successful synthetic-channel enqueue, and
filter later synthetic repeats before dispatch without granting overlap
credit. Clear it on a new Begin, successful global Live entry, or Deactivate.
A returned GetBlocks page must be fully dispatched before the engine observes
overlap or abandons its suffix.

### VSPC

VspcProcessor owns synthetic priority, notification retention, lower-bound
filtering, structural crossing, overlap proof, and its component-local Live
behavior; see
[vspc-processing.md](vspc-processing.md#catchup-filtering-crossing-and-overlap--settled)
and [vspc-processing.md](vspc-processing.md#component-local-live-transition--settled).
ResyncEngine only decides when to end synthetic production and enqueue that
component's Live command.

### Ordinary eligibility and coverage admission

At a fully dispatched GetBlocks-page boundary:

```text
PostSeal == true
&& block_overlap == true
&& vspc_overlap == true
```

establishes ordinary Live eligibility. It authorizes VspcProcessor Live and
the block-coverage phase, but not BlockProcessor Live or global `EnteredLive`.

Stop issuing VSPC V2 calls and sending new synthetic changes, stop/join that
producer, and successfully enqueue VspcProcessor's existing Live command
before the first coverage request. BlockProcessor remains in Catchup.
VspcProcessor's reaction and readiness behavior are defined in its focused
contract; the coverage phase sends it no checkpoint, barrier, or coverage
state.

Capture a fixed `GetBlockDagInfo.tip_hashes` set `T` after subscriptions are
Enabled and determine in one batch its strictly materialized subset `M`. Only
the tip vector is the coverage snapshot; other response fields are not treated
as one atomic combined snapshot. Do not refresh `T`. The pinned upstream
ordering commits each body-tip-store update before emitting its BlockAdded
notification, so blocks committed after the snapshot are protected by the
active subscription.

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
orphans to finish before Live. If the cap is exhausted without satisfying the
invariant, request `Require(Resync)`. The next recovery attempt derives its
starting sink from current committed database state.

On Live:

- VspcProcessor has already received Live at ordinary eligibility;
- stop and join the block coverage producer before sending BlockProcessor
  Live;
- BlockProcessor retains valid queued/orphan work.

`EnteredLive` is emitted only after the block coverage producer is
stopped/joined, the earlier VspcProcessor Live enqueue succeeded, and
BlockProcessor Live is successfully enqueued. It does not mean queues or
orphan state are empty. At this same lifecycle point, ResyncEngine sends
ApiService the Live publication trigger. If coverage exhausts its budget, do
not send BlockProcessor Live; request Resync and deactivate the component-local
VSPC Live run normally.

Normal processor and stream invariants remain active in Live and request their
settled recovery dispositions when violated.

## Teardown and delivery semantics — settled

On Deactivate, ResyncEngine performs this barrier in order:

1. close local routing gates and disable notifications;
2. cancel and join synchronization producers;
3. clear engine-local buffers;
4. send `Deactivate` to processors;
5. await processor acknowledgements, including descendant barriers;
6. release every processing-session clone of the validated RPC and DB handles;
7. drop `ProcessingSession`; and
8. emit `Deactivated` and enter Idle.

The owning services may retain their validated generations. Shutdown stops
engine and processors before NodeService and then StorageService. Channel
failures follow the bounded-delivery and ownership semantics above;
component-specific draining duties remain in the focused processor documents.

The graph observer path is deliberately separate: observer loss invalidates
and reloads the API image without interrupting processing. Its behavior is
defined in [api.md](api.md#in-process-api-and-graph-observer-feed--settled).
