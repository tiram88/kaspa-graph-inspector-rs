# Block processing

> Focused extraction; the current consolidated contract is
> [handoff-2026-09-20.md](handoff-2026-09-20.md), which prevails on conflicts.

## PP-boundary policy — settled

For a Rebuild in `PreSeal`:

```text
boundary_seal_blue_score =
    node_pp.blue_score + anticone_finalization_depth
```

This deliberately simple BlueScore approximation is accepted.

In `PreSeal`, blocks are processed in consensus-topological order
with `AllowBoundaryIdentities`; missing parents and merge-set identities may be
outside the retained PP boundary. The materialized retained portion of the PP
anticone arrives before PP-future blocks that merge it.

The first block with blue score at or above the threshold is processed under
strict policy. Only after that block commits does BlockProcessor enter `PostSeal`
and emit `PpBoundarySealed`.

After sealing, every resync cycle is strict. Before Catchup, a missing
dependency is a resync failure, not an orphan. In Catchup and Live, ordinary
orphans are allowed.

The observed retained PP anticone is complete enough for KGI visualization
semantics. No placeholder `blocks` rows and no placeholder promotion are
needed; outside-boundary identities live only in `block_identifiers`.

## BlockProcessor — settled

Single event loop priority:

1. commands;
2. intern/resolved blocks;
3. notification blocks;
4. synthetic blocks.

Tokio biased selection alone is insufficient as a total priority guarantee;
drain/check higher-priority channels deliberately around lower-priority work.

Conceptual commands include:

```text
BeginRebuild
BeginResync
PpBoundarySealed
Catchup { lower_bound, ... }
Live
Deactivate
Shutdown
```

A Begin command fully resets processor-local run state, including gates,
overlap tracking, orphan state, and descendants. Begin does not require an
acknowledgement because processing is not blocked on it. `Deactivate` is an
acknowledged barrier.

Notification gating:

- Begin closes the local notification gate.
- Catchup opens it.
- The notification receiver should still be polled while the gate is closed;
  received notifications are discarded immediately rather than accumulating
  for later ambiguity.

Materialization success, including dedup of an already materialized block,
returns its ID. BlockProcessor sends the resulting `PersistedBlock`
asynchronously to VspcProcessor and OrphanManager.

## OrphanManager — settled

OrphanManager is an asynchronous worker owned by BlockProcessor. It has
separate command and data-message channels for prompt lifecycle reactions.

It owns:

- in-memory orphan blocks;
- dependency topology;
- reverse missing-hash indexes;
- the `resolution_pending` set;
- selection of dependency requests;
- cancellation of requests made unnecessary by natural arrivals.

Messages include new orphan, `BlockPersisted`, dependency result/failure, and
capacity/topology updates. It sends newly ready blocks to BlockProcessor's
intern/resolved lane.

Dependency selection is topology-only. Age and DAA score are not selection
inputs. Start RPC requests at the orphan frontier when occupancy reaches
roughly one quarter or one third of capacity. Exact capacity and threshold are
implementation-phase decisions.

Notification loss is not treated as an ordinary silent event: disconnect or a
full bounded notification channel triggers recovery. Therefore no independent
age fallback is required merely to rescue an isolated orphan.

## DependencyResolver — settled

DependencyResolver is owned by BlockProcessor for lifecycle purposes but is
driven by OrphanManager.

- separate command and work-message channels;
- exact session `Arc<ValidatedRpcClient>`;
- bounded concurrent `GetBlock` tasks;
- no database transactions in `GetBlock` calls;
- no duplicate requests for hashes in OrphanManager's pending set;
- task cancellation capability;
- OrphanManager sends `Cancel(hash)` when the block arrives naturally;
- results go to BlockProcessor's intern/resolved channel;
- deactivation cancels/joins tasks and releases all session RPC clones before
  acknowledging.

There is no `Satisfied(hash)` queue protocol. `resolution_pending` is a set,
not a queue or map.
