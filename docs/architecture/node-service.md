# NodeService connectivity and notification routing

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
connection alone does not reset it. Network mismatch, incompatible RPC API,
and missing required notification capabilities publish terminal `Rejected`
and are not retried under unchanged configuration. Invalid local consensus
parameter configuration fails startup as defined below.

Validation requires:

- exact configured Kaspa network type and suffix;
- discovery of the node's configured Genesis hash through the validated RPC
  connection;
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
    genesis_hash: BlockHash,
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

### Consensus parameter resolution

The public RPC surface does not expose the node's effective consensus
parameters. `NetworkId` and the RPC-discovered Genesis hash therefore validate
network identity, but do not prove that the node uses KGI's local parameter
values. `KgiConsensusParams` is an explicit local assumption used for recovery,
not a node-validated property.

After validating the exact `NetworkId`, resolve local rusty-kaspa `Params` as
follows:

```text
mainnet
    -> Params::from(network_id)

locally supported testnet NetworkId
    -> Params::from(network_id)
    -> warn and continue

unsupported testnet suffix
    -> Params::from(NetworkType::Testnet)
    -> warn and continue

devnet or simnet with --override-params-file <path>
    -> Params::from(network_id)
    -> parse rusty-kaspa OverrideParams from path
    -> Params::override_params(overrides)
    -> warn that equality with the node cannot be verified

devnet or simnet without --override-params-file
    -> Params::from(network_id)
    -> warn that node overrides cannot be detected
```

At the pinned rusty-kaspa revision, `Params::from(NetworkId)` panics for an
unsupported testnet suffix rather than returning an error. The resolver checks
local support before calling it and uses the testnet-family fallback only for
an unsupported testnet suffix; panic catching is not control flow.

`--override-params-file` is valid only for configured devnet or simnet. It uses
the same JSON `OverrideParams` format as rusty-kaspa and is loaded once at
process startup. It is explicitly unsupported for mainnet and every testnet
suffix. An explicitly supplied file that is unreadable, malformed, or
incompatible, or use of the option with an unsupported network, is a
configuration error and never falls back silently. The file is not persisted
in node metadata. Genesis remains RPC-discovered and is not taken from the
file.

Obtain the copied values from the resolved `Params` through `bps()`,
`mergeset_size_limit()`, and `anticone_finalization_depth()`; do not reproduce
either upstream formula inside KGI. Every non-mainnet network emits a
divergence warning that logs the exact `NetworkId`, the parameter source, and
all three values KGI will use, then processing continues. Mainnet does not emit
this warning because the official rusty-kaspa daemon rejects parameter
overrides there. The
[PUAR](verification.md#current-puar-result) checks local resolution and values
at the reference revision. It does not claim equality with a connected node or
custom build. Correct processing requires the node's effective values to match
the selected local values; divergence is an accepted operator risk rather than
a detectable runtime rejection.

### Genesis discovery

Genesis identity comes from the node, not from KGI's local consensus
parameters. This keeps identity correct for custom devnets and new network
suffixes whose Genesis hash KGI may not know. It does not validate the local
consensus parameter assumption above.

During validation of each physical RPC generation, NodeService makes this raw
request outside the ordinary normalized block pump:

```text
GetBlocks {
    low_hash: None,
    include_blocks: false,
    include_transactions: false,
}
```

KGI relies on `None` selecting the node's configured Genesis as the low hash
and the first returned `block_hashes` member being that Genesis. The
[PUAR](verification.md#current-puar-result) checks this upstream assumption
against the reference revision. Validation requires a nonempty hash vector and
an empty block vector, then copies the first hash into
`ValidatedNodeInfo.genesis_hash`. Transport failure or malformed output fails
that validation attempt; NodeService never guesses or substitutes a locally
known Genesis hash.

This discovery call is deliberately distinct from `get_blocks` normalization
below. It neither constructs a synchronization page nor applies the explicit
inclusive-low-hash contract used by the pump.

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
precondition: ResyncEngine has sent Catchup to both processors for this run
start BlockAdded remotely
start VirtualChainChanged remotely
enable router only when both succeeded
publish subscription Enabled only when both succeeded
```

Catchup is a caller-established lifecycle precondition, not a NodeService
operation. ResyncEngine is the sole sender of processor commands and invokes
NodeService subscription activation only after satisfying that precondition.
NodeService owns the four activation steps following it.

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
overlap credit and do not themselves request recovery. NodeService does not
replay dropped callbacks. The consequences for Live admission and a block that
later becomes required belong to the
[processing lifecycle](processing-lifecycle.md#recovery-scope-and-omitted-body-tips).
Disabling is an immediate local cutoff, not a quiescence or transport fence.

The [PUAR](verification.md#current-puar-result) establishes that virtual
processing can emit a fully empty VirtualChainChanged notification when
processing a side block does not move the selected-chain sink. This
notification filter is distinct from handling an empty VSPC V2 RPC page in the
synchronization pump. The reviewed sink selection cannot produce a notification
with a nonempty removed chain and an empty added path; that shape is the fault
above, not another no-op.

In a short Live IBD episode, connection loss or a violated stream invariant
already causes recovery. NodeService has no separate continuous-IBD mode in
KGI v2.

Only NodeService sees raw rusty-kaspa notification types. Outside NodeService,
the normalized payload is the shared
[`VspcChange`](domain-model.md#vspc-value-types--settled).

### RPC normalization

#### Current pruning-point block

The current pruning point is obtained through one normalized composite
operation on the run's exact validated generation:

```rust
impl ValidatedRpcClient {
    async fn current_pruning_point_block(
        &self,
    ) -> Result<SharedNodeBlock, NodeError>;
}
```

The operation calls `GetBlockDagInfo`, requires the response network to equal
`ValidatedNodeInfo.network_id`, reads a non-ORIGIN `pruning_point_hash`, and
then calls `GetBlock(pruning_point_hash, include_transactions = false)` on the
same generation. The returned block hash must equal `pruning_point_hash`, and
the header plus verbose data must supply the direct parents, selected parent,
ordered blue/red merge sets, DAA score, blue score, and blue work required by
reconciliation and `rebuild_from_pruning_point`. Transactions are not required.
The [processing lifecycle](processing-lifecycle.md) owns when Resync and Rebuild
invoke this operation and how they consume its normalized result.

A mismatched response network, ORIGIN pruning-point hash, wrong returned block
hash, definitive not-found for the advertised pruning point, or missing required
block fields is
`RecoveryInputInvalid(MalformedPruningPointResponse)`. The exact validated RPC
generation is retired and the shared malformed recovery-input policy applies.
A transport failure, cancellation, or generation loss remains a session fault
and does not establish a reconciliation mismatch.

#### GetBlocks and VSPC recovery responses

`get_blocks(low_hash, include_blocks = true)` is normalized inside NodeService:

- raw hash and block vectors must have equal length;
- the raw response must be nonempty and start with `low_hash`;
- hashes must match returned blocks;
- duplicate hashes are invalid;
- strip the inclusive `low_hash` entry;
- accept zero normalized blocks after stripping that entry;
- return `Vec<SharedNodeBlock>`; a parallel hash vector is unnecessary.

Unequal vectors, an empty raw response, a first hash other than `low_hash`, a
hash/block disagreement, or any duplicate is
`RecoveryInputInvalid(MalformedGetBlocks)` when encountered by the recovery
pump. A valid anchor-only response that normalizes to zero blocks is not
malformed.

KGI requests full RPC blocks directly. Fetching hashes and then calling
`GetBlock` one by one has no accepted benefit for this local-node deployment.
DependencyResolver uses individual `GetBlock` calls for missing dependencies
without holding database transactions.
For `GetVirtualChainFromBlockV2`, request
`min_confirmation_count = None` and
`data_verbosity_level = Some(RpcDataVerbosityLevel::None)`. KGI relies on this
combination preserving a minimal acceptance-data envelope and an advancing
`added.last()` cursor. The RPC's exact added chain-path batch size is
`10 * mergeset_size_limit`. This limit bounds `added`, while the complete
`removed` suffix is not batch-limited. The same numeric budget bounds merged
blocks loaded for acceptance data; the resulting acceptance-data length may
shorten `added`, but only to a complete prefix. The
[PUAR](verification.md#current-puar-result) checks these upstream assumptions
against the reference revision; KGI-owned request construction and response
handling are covered by
[verification.md](verification.md#nodeservice-and-rpc-behavior).

For recovery VSPC V2, a fully empty response is a valid pump hint. Classify
response violations precisely:

```text
empty added with nonempty removed -> RemovedChainWithoutAddedPath
added.last() equals low_hash      -> NonAdvancingAddedCursor
duplicate within either vector    -> DuplicateChainMember
hash present in both vectors      -> RemovedAddedIntersection
```

Each is
`RecoveryInputInvalid(MalformedVspcResponse(reason))` and follows the shared
malformed recovery-response policy.

#### Individual recovery GetBlock

During Resync preparation, `GetBlock(sink_hash, false)` must return exactly the
requested hash and the header data required to construct
`MaterializedSyncAnchor`. A different hash or missing required GhostDAG header
data is `RecoveryInputInvalid(MalformedGetBlock)`. A definitive not-found
response and a returned DAA score that disagrees with committed storage remain
reconciliation evidence under the processing-lifecycle contract rather than
malformed transport shapes.

### Runtime protocol violation and generation retirement

`ValidatedRpcClient` recognizes malformed raw recovery RPC responses while
validating and normalizing them and returns the typed violation without a
normalized value. The calling recovery owner reports that violation to
NodeService before reporting its owner-directed fault. NodeService atomically
retires that exact published
`ValidatedRpcClient`, retires its NotificationRouter, prevents all clones from
starting further RPC or subscription work, closes the physical connection, and
enters `Unavailable`. A stale report for a generation already replaced cannot
retire the replacement. NodeService reconnects and performs complete validation
before publishing another generation.

The malformed response is never retried in place. Supervisor owns the shared
cross-generation retry budget and Fatal threshold defined by the
[processing lifecycle](processing-lifecycle.md#supervisor-and-recovery-intent--settled).
Malformed Genesis-discovery output occurs before a validated generation is
published and remains an ordinary NodeService validation failure under its
connection backoff; it does not consume this runtime malformed-input budget.
