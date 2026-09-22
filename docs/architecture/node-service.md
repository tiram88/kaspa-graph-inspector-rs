# NodeService connectivity and notification routing

> Focused extraction; the current consolidated contract is
> [handoff-2026-09-20.md](handoff-2026-09-20.md), which prevails on conflicts.

## NodeService — settled

`NodeService` is a permanent autonomous lifecycle worker. It owns connection,
reconnection, validation, IBD waiting, and publication of usable connection
generations.

```text
NodeService lifecycle worker
    -> connect
    -> validate
    -> wait for node to leave IBD
    -> publish Arc<ValidatedRpcClient>
```

`ValidatedRpcClient` represents exactly one validated physical connection
lifetime. Processing code receives this capability, not `NodeService`.

Validation requires:

- exact configured Kaspa network type and suffix;
- compatible RPC API version/revision;
- `handle_stop_notify() == true`;
- `handle_message_id() == true`;
- node is not in IBD when the handle is published;
- gRPC automatic reconnect is disabled.

Legacy Go kaspad notification semantics are deliberately unsupported.

`ValidatedNodeInfo` copies only KGI-relevant values rather than retaining the
full rusty-kaspa params object:

```rust
struct ValidatedNodeInfo {
    network_id: NetworkId,
    server_version: String,
    rpc_api_version: Option<u16>,
    rpc_api_revision: Option<u16>,
    consensus: KgiConsensusParams,
}

struct KgiConsensusParams {
    bps: u64,
    mergeset_size_limit: u64,
    anticone_finalization_depth: u64,
}
```

Add copied parameters only when KGI behavior actually depends on them.
After validating the exact `NetworkId`, obtain these values from the pinned
rusty-kaspa `Params` for that network through `bps()`,
`mergeset_size_limit()`, and `anticone_finalization_depth()`. Reject unsupported
network suffixes before constructing `Params`; do not reproduce the
merge-set-limit formula inside KGI.

### NotificationRouter

`NotificationRouter` implements rusty-kaspa's `Notify` trait and owns the two
bounded senders to the processors.

Router states:

```text
Disabled | Enabled | Retired
```

- Disabled drops notifications immediately.
- Enabled routes with nonblocking bounded sends.
- A full destination channel means notification loss: disable both streams and
  raise a recovery fault.
- An Enabled `BlockAdded` without `block.verbose_data` cannot provide the
  selected parent and merge sets required for materialization and reports
  `Require(Resync)`.
- Before attempting the bounded VSPC send, classify a raw
  VirtualChainChanged notification with empty `added`. When `removed` is also
  empty, discard the valid upstream no-op: it consumes no channel capacity,
  earns no overlap credit, changes no committed state, and requests no
  recovery. When `removed` is nonempty, do not enqueue it; disable routing and
  report `Require(Resync)` because the notification violates selected-sink
  monotonicity.
- Retired never routes again.

The subscription state belongs to `ValidatedRpcClient` and cannot outlive its
connection. The handle is clonable through `Arc` and protects notification
state with a private mutex.

Enable ordering:

```text
send Catchup to both processors
start BlockAdded remotely
start VirtualChainChanged remotely
enable router only when both succeeded
publish subscription Enabled only when both succeeded
```

Disable ordering:

```text
disable router immediately
stop both remote subscriptions
```

Subscription changes are all-or-nothing. Partial failure makes the connection
state uncertain and retires the validated handle if rollback cannot be
proven. Callbacks received while the router remains Disabled during remote
activation are intentionally dropped; they receive no Catchup overlap
credit and do not themselves request recovery. Synthetic GetBlocks/VSPC
production continues until ordinary overlap is demonstrated. Before Live,
ResyncEngine captures a fixed body-tip snapshot and enforces the bounded
coverage invariant defined in `processing-lifecycle.md`; that gate accounts
for activation-time BlockAdded drops, including a block outside the
then-selected past. ResyncEngine stops synthetic VSPC production for that
block-only coverage and sends VspcProcessor its existing Live command;
BlockProcessor remains in Catchup. NodeService does not replay dropped
callbacks.
Disabling is an immediate local cutoff, not a quiescence or transport fence.

Pinned rusty-kaspa inspection establishes that virtual processing can emit a
fully empty VirtualChainChanged notification when processing a side block
does not move the selected-chain sink. This notification filter is distinct
from handling an empty VSPC V2 RPC page in the synchronization pump. Pinned
sink selection cannot produce a removed-only notification; that shape is the
fault above, not another no-op.

Only NodeService sees raw rusty-kaspa notification types. The only VSPC payload
outside NodeService is:

```rust
struct VspcChange {
    removed: Arc<[BlockHash]>,
    added: Arc<[BlockHash]>,
}
```

### RPC normalization

`get_blocks(low_hash, include_blocks = true)` is normalized inside NodeService:

- raw hash and block vectors must have equal length;
- the raw response must be nonempty and start with `low_hash`;
- hashes must match returned blocks;
- duplicate hashes are invalid;
- strip the inclusive `low_hash` entry;
- return `Vec<SharedNodeBlock>`; a parallel hash vector is unnecessary.

KGI requests full RPC blocks directly. Fetching hashes and then calling
`GetBlock` one by one has no accepted benefit for this local-node deployment.
For `GetVirtualChainFromBlockV2`, request
`min_confirmation_count = None` and
`data_verbosity_level = Some(RpcDataVerbosityLevel::None)`. Rusty-kaspa master
`c338d495` preserves a minimal acceptance-data envelope and an advancing
`added.last()` cursor for this combination; retain a pinned regression fixture.
