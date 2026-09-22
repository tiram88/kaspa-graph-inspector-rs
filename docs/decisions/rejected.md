# Rejected designs

These proposals were considered and not accepted. The linked current owner
defines the operative behavior.

| Rejected proposal | Reason or current direction |
|---|---|
| Go KGI dependency recursion is depth-limited. | The claim is false; `ProcessBlockAndDependencies` covers arbitrary depth. |
| One monolithic block and VSPC worker. | Separate processors preserve clear ownership and immediate committed block identity delivery. |
| Commit VSPC strictly by arrival order, or rewind and replay recent VSPC history. | Pending history, continuity, readiness, and recovery govern ordering in [VSPC processing](../architecture/vspc-processing.md). |
| Persist ordinary orphan-only hashes. | Ordinary orphan topology remains in memory; only retained identities follow the storage contract. |
| Use broad notification epochs or a timer-based latecomer grace period. | Transport packets cannot be assigned reliable epochs; Begin/Catchup gates and objective lower bounds handle late delivery. |
| Use local or adjacent GetBlocks order decrease as a Catchup trigger. | Only the complete page's global maximum position participates in the accepted order fallback. |
| Add a dedicated mixed-view recovery protocol for GetBlocks assembled while Virtual moves. | A material strict-processing omission uses the ordinary `Require(Resync)` path. |
| Derive a destination from a removed-only VSPC change. | The shape violates the pinned upstream invariant and follows source-specific recovery policy. |
| Admit a wholly empty `VirtualChainChanged` to VspcProcessor. | NodeService drops this valid no-op before bounded delivery. |
| Enable local routing before both remote subscriptions start. | Routing remains Disabled through both starts; activation callbacks are intentionally dropped. |
| Require zero unresolved orphans before Live. | Valid queued and orphan dependency work may remain after gapless admission. |
| Add a VSPC coverage phase, freeze acknowledgement, checkpoint, or synthetic-stream terminal marker. | VspcProcessor receives its existing Live command at ordinary eligibility; block coverage continues independently. |
| Escalate an arbitrary count of failed Resync attempts to Rebuild. | Recovery strength follows typed evidence rather than retry count. |
| Retry services or recovery immediately or without a rate bound. | The lifecycle contracts define capped jittered schedules and terminal rejection. |
| Transparently retry an ambiguous database commit. | Only definite `40001` and `40P01` rollbacks permit bounded whole-transaction retry. |
| Persist a dedicated VSPC checkpoint or sink table. | The committed sink is derived from retained block state. |
| Persist PP identity separately by CompactId. | Database PP is the retained block at `(1, 0)`. |
| Add committed-index waiters. | Commit and dedup return the block ID and BlockProcessor delivers `PersistedBlock` asynchronously. |
| Add `ReadyAddedBlock`, `ProcessingResources`, or a redundant `PendingVspcChange` wrapper. | Existing committed payloads, direct session resources, and `VspcChange` carry the required semantics. |
| Infer parent-row foreign-key validity merely from identity existence. | Storage validates materialized references transactionally. |
| Expose CompactId publicly or assume coordinates are stable across instances. | Public graph identity is the block hash; response-local numeric IDs are ephemeral. |
| Permit partially populated retained HGC levels. | Retained levels are complete or the image is unavailable. |
| Escalate graph-observer loss directly into processing recovery. | Observer loss invalidates and reloads the API image. |
| Serve or tolerate API reads from a partially rebuilt database, relying only on SQL errors. | The acknowledged Reset barrier prevents mixed-generation reads. |
| Move pruning-point or reconciliation preparation into Supervisor. | ResyncEngine owns preparation; Supervisor owns recovery intent. |
