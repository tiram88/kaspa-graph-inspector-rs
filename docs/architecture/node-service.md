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
    anticone_finalization_depth: u64,
}
```

Add copied parameters only when KGI behavior actually depends on them.

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
- Retired never routes again.

The subscription state belongs to `ValidatedRpcClient` and cannot outlive its
connection. The handle is clonable through `Arc` and protects notification
state with a private mutex.

Enable ordering:

```text
enable router
start BlockAdded remotely
start VirtualChainChanged remotely
publish Enabled only when both succeeded
```

Disable ordering:

```text
disable router immediately
stop both remote subscriptions
```

Subscription changes are all-or-nothing. Partial failure makes the connection
state uncertain and retires the validated handle.

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
