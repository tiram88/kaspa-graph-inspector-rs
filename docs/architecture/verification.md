# Verification contract

## Scope and evidence rules

This document owns the minimum evidence required for the KGI v2 architecture.
It does not redefine component behavior: each requirement below links to the
focused contract whose observable result it verifies.

Use the narrowest useful test level for behavior owned by KGI, but retain
integration coverage wherever the result depends on PostgreSQL, browser graph
behavior, or a multi-worker ordering boundary. Inject clocks and jitter sources
when wall-clock behavior is under test. Upstream correctness assumptions use
the source-analysis policy below rather than executable fixtures created only
to prove rusty-kaspa behavior.

These obligations are a required baseline. The breadth of an exhaustive parity
matrix remains deferred in [deferred.md](../decisions/deferred.md).

## Pinned Upstream Assumption Review policy

The **Pinned Upstream Assumption Review**, abbreviated **PUAR**, is the specific
source analysis of KGI's upstream correctness assumptions against one exact
rusty-kaspa commit. The current pinned reference revision is:

```text
c338d495bec29e4dc8b5149f99e8db6fa916ed4a
```

This pin makes the architecture analysis reproducible. It identifies the
rusty-kaspa source used to assess KGI's assumptions; it is not a runtime
version restriction. KGI may connect to other node versions after ordinary
validation, but the PUAR provides no correctness claim for those versions or
for custom builds.

The PUAR checklist is:

1. `GetBlocks(None, false, false)` returns the configured Genesis hash first
   and returns no blocks.
2. Header-only `GetBlock` supplies the required GhostDAG data and establishes
   that the returned block is recognized as a GetBlocks low hash.
3. The `L + 1` GetBlocks core budget, consensus-topological ordering, page
   construction, global-maximum fallback premise, and the `< 3` normalized
   length sink-reaching premise match the Catchup design.
4. Virtual selected-sink behavior preserves the documented removed/added
   ordering, excludes changes with a nonempty removed chain and an empty added
   path, can emit the documented fully empty no-op, and never places Genesis in
   `added` or `removed`.
5. BlockAdded duplicate and verbose-data behavior matches NodeService and
   BlockProcessor assumptions, including the enrichment-failure form without
   verbose data.
6. Exact-network, unsupported-testnet fallback, and devnet/simnet override
   resolution produce the documented local `bps`, `mergeset_size_limit`, and
   `anticone_finalization_depth` values. The review records that RPC exposes no
   comparison with the node's effective overrides; it does not describe these
   local values as node-validated.
7. VSPC V2 with `min_confirmation_count = None` and
   `RpcDataVerbosityLevel::None` preserves the documented batching, complete
   removed suffix, minimal acceptance-data envelope, complete-prefix
   shortening, and advancing cursor behavior.

For each item, inspect the committed source at the full SHA and report one of:

```text
Confirmed
Not confirmed
Contradicted
```

Each result contains a concise conclusion, relevant rusty-kaspa paths and
symbols, and any material KGI impact or limitation. A PUAR is evidence about
the reference revision rather than proof across node versions. No fixture or
test is required solely to establish an upstream checklist item. Executable
tests remain required for KGI's handling of valid input and observable
violations.

The instruction **Run the Pinned Upstream Assumption Review** runs the complete
checklist against the current reference revision. **Run a PUAR against
rusty-kaspa `<full-sha>`** selects an explicit revision. The resulting dated,
non-normative report belongs in:

```text
docs/reviews/YYYY-MM-DD-rusty-kaspa-<short-sha>-assumptions.md
```

A report does not change the reference pin or architecture automatically.
`Not confirmed` or `Contradicted` is reported to Architecture before dependent
implementation proceeds. Changing the reference revision requires a new PUAR;
otherwise repeat the review only when a compatibility problem or relevant
upstream change gives a concrete reason.

## NodeService and RPC behavior

Verify the [NodeService contract](node-service.md#nodeservice--settled) and
[RPC normalization](node-service.md#rpc-normalization) with:

1. GetBlocks fixtures for the inclusive low hash, unequal hash/block vector
   lengths, hash/block disagreement, duplicates, and a valid response that
   normalizes to zero blocks. For every malformed recovery shape, assert typed
   rejection, no cursor or processor advancement, and retirement of the exact
   RPC generation.
2. Genesis discovery request construction with `low_hash = None`, blocks and
   transactions disabled, plus response validation for a nonempty hash vector,
   empty block vector, transport failure, and malformed output. Given a valid
   response, NodeService uses its first hash without substituting a local
   Genesis constant.
3. Consensus parameter resolution uses exact `NetworkId` parameters when
   supported. Mainnet emits no divergence warning. Every non-mainnet profile,
   including supported testnet and simnet, warns with the exact network,
   parameter source, and all three derived values, then continues. An
   unsupported testnet suffix uses testnet-family defaults. Devnet and simnet
   without `--override-params-file` use their defaults; with the option they
   parse and apply rusty-kaspa `OverrideParams`. An unreadable, malformed, or
   incompatible explicit file and use of the option with mainnet or any
   testnet suffix fail configuration without fallback. No path represents the
   selected local values as having been compared with the node.
4. Given a fully empty `VirtualChainChanged`, NodeService drops it before
   bounded delivery with no overlap credit, processor-capacity use, or
   recovery. Distinguish it from an empty VSPC V2 RPC page.
5. VSPC V2 request construction uses `min_confirmation_count = None` and
   `data_verbosity_level = Some(RpcDataVerbosityLevel::None)`. Mocked valid
   complete-prefix and advancing-cursor responses exercise KGI's pump behavior.
   Reject a removed chain without an added path, duplicate members,
   removed/added intersection, and a nonadvancing nonempty added cursor as the
   corresponding `MalformedVspcResponse` reason, with generation retirement
   and no cursor advancement.
6. Mocked notification and VSPC V2 responses with a nonempty removed chain and
   an empty added path. Assert the source-specific dispositions: a notification
   requires Resync; `ValidatedRpcClient` rejects a synthetic response as
   `MalformedVspcResponse(RemovedChainWithoutAddedPath)` without returning a
   normalized change, and ResyncEngine follows the shared malformed
   recovery-response policy without dispatch or cursor advancement.
7. The subscription activation order across the
   [processing lifecycle](processing-lifecycle.md#entering-recovery-phases) and
   [NotificationRouter](node-service.md#notificationrouter): ResyncEngine alone
   sends both processor Catchup commands before invoking NodeService activation;
   NodeService sends no processor command; routing remains Disabled until both
   remote starts succeed; and callbacks received during that interval are
   dropped without overlap credit or immediate recovery.

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
   never rebinds its network. Interactive initialization identifies the
   database and complete network binding without exposing credentials;
   noninteractive initialization requires explicit authorization. `--clear-db`
   may authorize first initialization but never claims or rebinds an existing
   incompatible schema.
5. One-shot administrative reinitialization requires explicit confirmation,
   replaces only a recognized KGI schema, binds the new Empty database to the
   validated `(network_id, genesis_hash)`, and exits. A changed declarative
   token performs the same reset once; restart with the stored token preserves
   the database. Unknown tables are never claimed or destroyed, and neither
   form can expose a partially recreated schema.
6. Advisory-lock contention rejects with `DatabaseAlreadyInUse`, and lock loss
   retires the DB generation. Compatible v2 migrations are ordered,
   transactional, revalidated, and finish before client publication. Cover
   newer-schema and v1/unsupported rejection, failed migration without client
   publication, and the prohibition on automatic down or online migration.
7. Storage may open, lock, and inspect Uninitialized contents before node
   validation, but only atomic publication of complete `NodeMetadata` crosses
   `Uninitialized -> Empty`. Inject a crash around this transaction and prove
   that no partially bound Empty state can appear.
8. Node metadata is non-null and distinguishes Empty from Genesis-anchored
   initialization despite both using `db_pp_blue_score = 0`. Reject the same
   `NetworkId` paired with a different Genesis rather than rebinding or
   rebuilding.
9. A Genesis anchor is materialized at `(1,0)`, belongs to VSPC, has ORIGIN as
   selected parent and zero actual direct parents, and has a coherent committed
   sink. Any additional boundary identity classifies processing contents as
   Inconsistent and requires Rebuild.
10. `rebuild_from_pruning_point` either publishes the complete replacement or
   leaves the previous contents intact: Compact-ID allocation restarts, the PP
   level receives its VSPC DAA score, no placeholder block rows exist, and
   replacement caches publish only after definite commit. An ambiguous commit
   retires the DB generation without reporting successful Rebuild.
11. Materiality remains derived from `blocks`; ordinary processing performs no
    individual block/identity deletion or boundary-identity promotion; parent
    rows have no foreign keys or cascade semantics; merge-set IDs validate
    transactionally; and coordinate uniqueness plus `levels.size` updates are
    enforced in the insertion transaction.

Verify the [block materialization transaction](storage.md#block-materialization-transaction--settled)
and [PP seal behavior](block-processing.md#pp-boundary-phase-behavior--settled)
with an ordering fixture for a non-Genesis PP: the threshold block commits
before BlockProcessor enters PostSeal or emits `PpBoundarySealed`. For Genesis,
verify that the rebuild transaction establishes an intrinsically sealed
boundary, `BeginRebuild` enters PostSeal directly, and the same exact-once
milestone follows Begin without ordinary Genesis materialization. Inject crash
and ambiguous-commit outcomes around both paths; an unproven non-Genesis seal
emits no milestone, while restart after a committed Genesis rebuild derives
PostSeal from storage and proceeds through ordinary Resync.

Verify the [atomic VSPC transaction](storage.md#atomic-vspc-transaction--settled)
for source continuity, direct-chain materiality, duplicate/intersection
rejection, atomic membership and coloring changes, merge-set members
represented only by boundary identity, and final level-score publication.

Verify [transaction retries](storage.md#transaction-retries) with PostgreSQL
integration fixtures. Only SQLSTATE `40001` and `40P01` retry the complete
transaction, using the nominal `10/50/250ms` slots. Assert no pre-commit cache
publication, no retry after an ambiguous commit, and the specified
recovery-versus-Live disposition after exhaustion. Exercise the inclusive 50%
through 100% jitter range.

## Recovery lifecycle and Catchup

Verify [Resync preparation](processing-lifecycle.md#resync-preparation) with
fixtures that combine stored sink ID/hash/selected-parent hash/DAA score with
an exact no-transactions GetBlock header. Cover:

- successful `MaterializedSyncAnchor` construction, including a header-only
  node block;
- exact propagation into both processor Begin payloads, including
  VspcProcessor initialization of its committed sink and history seed;
- a definitively absent sink, incoherent stored sink, and a DAA mismatch
  requiring Rebuild;
- transport or session failure without inferring Rebuild; and
- a response carrying the wrong hash or missing required GhostDAG header data
  as `MalformedGetBlock`, retiring the exact RPC generation without inferring
  Rebuild.

Also cover a coherent Genesis-anchored database below anticone finalization
depth, Empty-versus-Genesis discrimination, exact node/database
`(network_id, genesis_hash)` matching, and both synthetic streams anchored at
the committed materialized VSPC sink. Immediately after bootstrap that sink is
Genesis; after advancement it may be newer. ORIGIN must never be sent to an
RPC. A mismatch in immutable node identity is rejected rather than requesting
Rebuild.

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
and [global Live admission](processing-lifecycle.md#live-admission)
together. Required cases are:

1. Command priority, the Begin gate and immediate closed-gate drop, objective
   lower-bound filtering, both overlap flags, observation only at full-page
   boundaries, and Live entry while valid ordinary orphans remain.
2. Cross-response synthetic block repeats; add to the Catchup-only sent-hash
   set only after a successful enqueue and grant no overlap credit to a
   filtered repeat. This set does not participate in Live admission.
3. A temporary empty synthetic VSPC queue before component-local Live does not
   release a notification. The Live transition discards queued synthetic
   input and makes notifications authoritative for the rest of the session.
4. Notification-driven VSPC sink advancement and notification changes held in
   pre-resolution readiness by unfinished block dependencies.
5. At a complete GetBlocks-page boundary, satisfying PostSeal plus both overlap
   flags causes the lifecycle to stop and join both synthetic producers, then
   enqueue VspcProcessor Live before BlockProcessor Live and finally publish
   global Live.
6. Live admission does not call GetBlockDagInfo, wait for extra GetBlocks
   pages, or inspect body-tip materiality. A dropped activation callback for an
   otherwise unobserved stale body tip neither blocks Live nor requests
   recovery.
7. An omitted block that later appears as an admitted block dependency or a
   VSPC chain member follows the existing dependency or recovery disposition.

Verify the [fault and retry policy](processing-lifecycle.md#supervisor-and-recovery-intent--settled)
with injected clocks and deterministic jitter:

- whole-attempt recovery Retry rather than in-place page/RPC retry;
- the general delay sequence and 30-second cap;
- no second delay while awaiting a replacement service generation;
- the general Retry backoff resets on Live, a new resource generation, and a
  stronger obligation;
- recovery requirements do not consume Retry backoff;
- one shared counter across malformed GetBlock, GetBlocks, and VSPC response
  kinds and all VSPC reasons, including `RemovedChainWithoutAddedPath`: each of
  the first three occurrences retires its producing generation and retries only
  after replacement, the fourth is Fatal, replacement generations and stronger
  recovery do not reset the counter, `EnteredLive` does reset it, and
  NodeService reconnect delay is not combined with the general recovery Retry
  delay; and
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
- an actionable reorg resolving `removed` followed by `added` through one
  ordered, cache-first storage batch;
- strict-`<` sink-preserving history pruning before Catchup, at the
  Catchup-to-Live transition, and during Live, with pruning disabled throughout
  Catchup;
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

Observer and API projection tests cover a non-Genesis block whose selected
parent occurs exactly once in `direct_parents` with `is_selected = true`, plus
Genesis with an empty `direct_parents` list and no synthetic ORIGIN parent.
Genesis recognition must not require the public projection or Web client to
expose or consult persisted `NodeMetadata.genesis_hash`.

Browser graph tests cover the [Web contract](web.md): update acquisition,
fixed-view freeze and follow-live behavior, stable block identity, direct
parent rendering, and Genesis recognition from zero actual direct parents.

## Resource isolation and performance

API load tests verify the
[system isolation contract](overview.md#resource-isolation-and-scalability--settled)
and [API bulkheads](api.md#resource-isolation-and-saturation--settled).
Exercise status, head, and historical admission lanes independently, including
configured rejection limits, while measuring both processors' commit latency.
Also cover bounded slow-SSE behavior, cache-memory exhaustion with complete
levels, explicit rejection or temporary unavailability, and the required
operational measurements. Saturated API work must not materially delay
processing or produce a partial graph image.

Go KGI parity fixtures, rusty-kaspa RPC and notification fixtures, PostgreSQL
integration tests, and browser graph tests accompany the applicable groups
above.
