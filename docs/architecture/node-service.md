# NodeService connectivity and notification routing

> Focused extraction; during the documentation reorganization, the current
> consolidated contract remains
> [handoff-2026-09-20.md](handoff-2026-09-20.md), which prevails on conflicts.

## Scope and ownership

This document owns node connectivity, validated RPC generations, remote
subscription state, notification routing, and node-response normalization.
The [processing lifecycle](processing-lifecycle.md) owns recovery coordination
after NodeService reports a typed fault.

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

```rust
enum NodeServiceState {
    Connecting,
    Ready(Arc<ValidatedRpcClient>),
    Unavailable(NodeUnavailableReason),
    Rejected(NodeRejection),
    Stopped,
}
```

`NodeServiceState` and published status outlive individual validated client
generations. `wait_until_usable()` waits through transient `Connecting` and
`Unavailable` states, but returns permanent `Rejected`, `Stopped`, or closed
service state to its caller.

Transient connection, transport, and validation-RPC failures enter
`Unavailable` and are retried indefinitely with nominal delays:

```text
1s, 2s, 4s, 8s, 16s, 30s, 30s, ...
```

Each actual delay uses equal jitter from 50% through 100% of the nominal
delay, and every wait is shutdown-cancellable. Reset the sequence only after
NodeService has remained continuously Ready for 60 seconds; opening a
connection alone does not reset it. Network mismatch, unsupported network
suffix, incompatible RPC API, and missing required notification capabilities
publish terminal `Rejected` and are not retried under unchanged
configuration.

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
  report `Require(Resync)`.
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

Subscription changes are all-or-nothing. If either remote start fails, stop
any subscription already started; retire the validated handle when rollback
cannot be proven. A partial subscription or unsubscription failure likewise
retires the handle. Callbacks received while the router remains Disabled
during remote activation are intentionally dropped; they receive no Catchup
overlap credit and do not themselves request recovery. The fixed body-tip
coverage gate defined by the
[processing lifecycle](processing-lifecycle.md) accounts for these drops
before global Live. NodeService does not replay dropped callbacks.
Disabling is an immediate local cutoff, not a quiescence or transport fence.

Pinned rusty-kaspa inspection establishes that virtual processing can emit a
fully empty VirtualChainChanged notification when processing a side block
does not move the selected-chain sink. This notification filter is distinct
from handling an empty VSPC V2 RPC page in the synchronization pump. Pinned
sink selection cannot produce a removed-only notification; that shape is the
fault above, not another no-op.

In a short Live IBD episode, connection loss or a violated stream invariant
already causes recovery. NodeService has no separate continuous-IBD mode in
KGI v2.

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
- accept zero normalized blocks after stripping that entry;
- return `Vec<SharedNodeBlock>`; a parallel hash vector is unnecessary.

KGI requests full RPC blocks directly. Fetching hashes and then calling
`GetBlock` one by one has no accepted benefit for this local-node deployment.
DependencyResolver uses individual `GetBlock` calls for missing dependencies
without holding database transactions.
For `GetVirtualChainFromBlockV2`, request
`min_confirmation_count = None` and
`data_verbosity_level = Some(RpcDataVerbosityLevel::None)`. Rusty-kaspa master
`c338d495` preserves a minimal acceptance-data envelope and an advancing
`added.last()` cursor for this combination; retain a pinned regression fixture.
