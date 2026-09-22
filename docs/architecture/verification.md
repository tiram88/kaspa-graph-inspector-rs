# Verification contract

> Focused extraction; the current consolidated contract is
> [handoff-2026-09-20.md](handoff-2026-09-20.md), which prevails on conflicts.

## Scope and evidence rules

This document owns the minimum executable evidence required for the KGI v2
architecture. It does not redefine component behavior: each requirement below
links to the focused contract whose observable result it verifies.

Use the narrowest useful test level, but retain integration coverage wherever
the result depends on PostgreSQL, rusty-kaspa RPC behavior, browser graph
behavior, or a multi-worker ordering boundary. Inject clocks and jitter sources
when wall-clock behavior is under test. Critical upstream assumptions must be
pinned to the chosen rusty-kaspa revision and fail visibly when that revision
changes incompatibly.

These obligations are a required baseline. The breadth of an exhaustive parity
matrix remains open in [open-questions.md](../open-questions.md).

## Node and upstream RPC fixtures

Verify the [NodeService contract](node-service.md#nodeservice--settled) and
[RPC normalization](node-service.md#rpc-normalization) with:

1. GetBlocks fixtures for the inclusive low hash, unequal hash/block vector
   lengths, hash/block disagreement, duplicates, and a valid response that
   normalizes to zero blocks.
2. A pinned rusty-kaspa fixture in which a side-block task emits a fully empty
   `VirtualChainChanged`; assert that NodeService drops it before bounded
   delivery with no overlap credit, processor-capacity use, or recovery.
   Distinguish it from an empty VSPC V2 RPC page.
3. A pinned fixture for
   `min_confirmation_count = None` and
   `data_verbosity_level = Some(RpcDataVerbosityLevel::None)` that checks the
   minimal acceptance-data envelope, any acceptance-data length limit, and an
   advancing `added.last()` cursor for every nonempty response.
4. Mocked removed-only notification and VSPC V2 responses. Assert the
   source-specific dispositions: a notification requires Resync; a synthetic
   page does not advance its cursor and uses the bounded whole-attempt Retry
   policy.
5. The subscription activation order in
   [NotificationRouter](node-service.md#notificationrouter): routing remains
   Disabled until both remote starts succeed, callbacks received during that
   interval are dropped, and neither overlap credit nor immediate recovery is
   produced.

Use injected clocks and deterministic jitter to verify the independent
[NodeService](node-service.md#nodeservice--settled) and
[StorageService](storage.md#storageservice-lifecycle--settled) reconnect
sequences: nominal exponential slots through the 30-second cap, reset only
after 60 seconds continuously Ready, shutdown cancellation, and terminal
rejection without retry. Exercise the inclusive 50% through 100% jitter range.

## Domain, storage, and PostgreSQL

Verify the [PP boundary model](domain-model.md#pruning-point-boundary-and-origin--settled),
[persistent representation](storage.md#persistent-representation--settled),
and [rebuild transaction](storage.md#rebuild-transaction--settled) with:

1. Rebuild materializes PP at `(level = 1, slot = 0)`; retained anticone roots
   and nonroots follow the parent-derived placement rule; boundary identities
   never become block rows.
2. Genesis and any PP whose selected parent is ORIGIN retain a non-null ORIGIN
   identity without inventing a direct parent. Web Genesis detection uses the
   actual absence of direct parents rather than visible-edge absence.
3. Hash interning follows the universal five-stage order. Strict
   materialization creates no boundary identity.
4. Database startup distinguishes Uninitialized, Empty, Initialized,
   structurally inconsistent processing contents, and rejected schemas.
   Persistent `--initialize-db` is idempotent for a compatible database and
   never rebinds its network.
5. Advisory-lock loss retires the DB generation. Compatible v2 migrations are
   transactional and finish before client publication.

Verify the [block materialization transaction](storage.md#block-materialization-transaction--settled)
and [PP seal behavior](block-processing.md#pp-boundary-phase-behavior--settled)
with an ordering fixture: the threshold block commits before BlockProcessor
enters PostSeal or emits `PpBoundarySealed`. Inject crash and ambiguous-commit
outcomes around the seal and assert that no unproven milestone is emitted and
the next run derives truth solely from committed storage.

Verify the [atomic VSPC transaction](storage.md#atomic-vspc-transaction--settled)
for one ordered cache-first reorg-member ID batch, strict-`<` sink-preserving
pruning, disabled Catchup pruning, merge-set members represented only by
boundary identity, and atomic publication of level scores.

Verify [transaction retries](storage.md#transaction-retries) with PostgreSQL
integration fixtures. Only SQLSTATE `40001` and `40P01` retry the complete
transaction, using the nominal `10/50/250ms` slots. Assert no pre-commit cache
publication, no retry after an ambiguous commit, and the specified
recovery-versus-Live disposition after exhaustion. Exercise the inclusive 50%
through 100% jitter range.

## Recovery lifecycle and Catchup

Verify [Resync preparation](processing-lifecycle.md#resync-preparation) with
fixtures that combine stored sink ID/hash/DAA score with an exact
no-transactions GetBlock header. Cover:

- successful `MaterializedSyncAnchor` construction, including a header-only
  node block;
- a definitively absent or invalid sink and a DAA mismatch requiring Rebuild;
- transport or session failure without inferring Rebuild; and
- a response carrying the wrong hash as a protocol/session fault.

Verify the [Catchup trigger](processing-lifecycle.md#catchup-trigger) with:

1. Rolling marker replacement, replacement already present in the held page,
   cursor equality, and `Unknown -> Present -> Removed` tracking.
2. Checked and decreasing DAA-score cases and the page-aware threshold at 1,
   10, and 32 BPS. Catchup must precede dispatch of the complete held page only
   when its VSPC-established marker satisfies the contract.
3. Independent fixtures for the global-maximum-position fallback, normalized
   GetBlocks length below three, and empty VSPC V2 page.
4. A material pre-Catchup omission using the existing strict-processing
   `Require(Resync)` path.
5. The upward seal milestone path from BlockProcessor through ResyncEngine to
   Supervisor. It never returns to BlockProcessor as a command, and no Catchup
   path is admitted before ResyncEngine observes it; global Live still requires
   PostSeal.

Verify [phase entry](processing-lifecycle.md#entering-recovery-phases),
[processor-local block overlap](block-processing.md#catchup-filtering-and-overlap),
[processor-local VSPC overlap](vspc-processing.md#catchup-filtering-crossing-and-overlap--settled),
and [global Live admission](processing-lifecycle.md#ordinary-eligibility-and-coverage-admission)
together. Required cases are:

1. Command priority, the Begin gate and immediate closed-gate drop, objective
   lower-bound filtering, both overlap flags, observation only at full-page
   boundaries, and Live entry while valid ordinary orphans remain.
2. Cross-response synthetic block repeats; add to `catchup_sent` only after a
   successful enqueue and grant no overlap credit to a filtered repeat.
3. A temporary empty synthetic VSPC queue before component-local Live does not
   release a notification. The Live transition discards queued synthetic
   input and makes notifications authoritative for the rest of the session.
4. Notification-driven VSPC sink advancement and notification changes held in
   pre-resolution readiness by unfinished block dependencies.
5. A fixed body-tip snapshot and one batch of its strictly materialized subset.
   Pin the upstream premises that tips come from the body-tip store and that
   its update commits before BlockAdded emission.
6. `T ⊆ M ∪ catchup_sent` with an already materialized side tip, a dropped
   activation callback outside the then-selected past, a tip admitted on the
   final permitted page, and a tip that remains uncovered.
7. Coverage request starts obey the `1 / bps` minimum interval. Page caps are
   2, 3, and 3 at 1, 10, and 32 BPS with merge-set limits 180, 248, and 512;
   empty and fully filtered responses consume budget.
8. Success sends BlockProcessor Live only after a complete page and emits
   `EnteredLive` only after both processors' split Live enqueues. Cap exhaustion
   sends no BlockProcessor Live, requests Resync, and starts the next run from
   current committed DB state.

Verify the [fault and retry policy](processing-lifecycle.md#supervisor-and-recovery-intent--settled)
with injected clocks and deterministic jitter:

- whole-attempt recovery Retry rather than in-place page/RPC retry;
- the general delay sequence and 30-second cap;
- no second delay while awaiting a replacement service generation;
- resets on Live, new resource generation, and stronger obligation;
- recovery requirements do not consume Retry backoff;
- three removed-only synthetic retries on one RPC generation followed by
  Fatal on the fourth, with reset only on a new RPC generation or
  `EnteredLive`; and
- typed fault kinds, never diagnostics, drive policy and counters.

Verify [teardown](processing-lifecycle.md#teardown-and-delivery-semantics--settled)
by asserting that session-scoped RPC and DB clones are released before
`Deactivated`, cancellation races finish, and full versus closed bounded
channels retain their distinct dispositions.

## Block processing and dependency resolution

Verify [block admission](block-processing.md#admission-and-materialization),
[committed delivery](block-processing.md#committed-block-delivery),
[OrphanManager](block-processing.md#orphanmanager--settled), and
[DependencyResolver](block-processing.md#dependencyresolver--settled) with:

1. Reverse orphan topology releases every newly ready child.
2. Resolver pending state remains until `AddOrphan` or `BlockPersisted`.
3. Resolver cancellation and deactivation are safe with full child-result
   channels.
4. A resolver-confirmed unavailable dependency requires Rebuild, whereas RPC
   or connection failure does not establish unavailability.
5. A failed post-commit `PersistedBlock` delivery cannot be treated as a
   harmless duplicate or omission.

## VSPC processing

Verify [change semantics](vspc-processing.md#change-semantics--settled),
[pending history](vspc-processing.md#pending-state-and-history--settled),
[readiness](vspc-processing.md#readiness-and-materiality--settled), and
[Catchup crossing](vspc-processing.md#catchup-filtering-crossing-and-overlap--settled)
with:

- removed/added source and destination derivation;
- the added-only path with no database source;
- bounded pending order where one unready candidate does not block a later
  actionable candidate;
- structural crossing through `added` only;
- destination-equal discard without creating an empty normalized change;
- absence of overlap credit for unresolved or filtered candidates;
- missing nonretained merge-set members at the PP boundary; and
- direct nonmaterialized chain members requiring Rebuild.

## API and Web

Verify the [graph observer feed](api.md#in-process-api-and-graph-observer-feed--settled)
and [snapshot contract](api.md#snapshot-revision-delta-sse-and-etags--settled)
for causal order, a loss flag even when the terminal message is lost,
snapshot/replay with a represented VSPC prefix, source continuity, complete
cached levels, external parent-edge endpoints and level sizes, actual parent
presence independent of visible edges, and one Reset-driven replacement
GraphEpoch per prepared processing session.

Verify [DAA navigation and graph windows](api.md#daa-navigation-and-graph-windows--settled)
for floor selection and tie break, the sentinel result, a reorg-created
VSPC-empty level, atomic level-score publication, and navigation plus window
consistency from one image.

Verify deltas and client behavior across
[API publication](api.md#snapshot-revision-delta-sse-and-etags--settled) and
[Web update acquisition](web.md#update-acquisition--settled): sequential delta
composition and expiry, response-local hash dictionaries,
`Stale -> Synchronizing -> Live`, same-epoch atomic Live revision, SSE slow
clients, fixed-view freeze, DAA focus, and Live arriving during PostSeal load.
The latter must publish the completed image directly as Live.

Verify [Reset and recovery-time availability](api.md#reset-and-recovery-time-availability--settled)
with ordinary Resync and Rebuild integration scenarios. Resync preserves
historical reads and orders Reset before PostSeal and processor Begin. Rebuild
acknowledges Reset before DB clear, closes historical reads, permits only an
old coherent in-flight result or a clean 503, and reopens reads after coherent
PostSeal publication. Include query/reset races and PostgreSQL `TRUNCATE`; no
request may observe a partial or mixed generation.

Browser graph tests cover the [Web contract](web.md): update acquisition,
fixed-view freeze and follow-live behavior, stable block identity, direct
parent rendering, and Genesis recognition from zero actual direct parents.

## Resource isolation and performance

API load tests verify the
[resource isolation contract](handoff-2026-09-20.md#18-resource-isolation-and-scalability--settled).
Exercise status, head, and historical admission lanes independently, including
configured rejection limits, while measuring processor commit latency.
Saturated API work must not materially delay processing.

Go KGI parity fixtures, rusty-kaspa RPC and notification fixtures, PostgreSQL
integration tests, and browser graph tests accompany the applicable groups
above.
