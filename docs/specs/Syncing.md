# Syncing

## Implementation Overview

### Reth Sync Overview

When a Reth node falls behind other Reth nodes while the consensus layer (CL) is not advancing, Reth continues to receive new blocks through the P2P networking layer (`crates/net/`). 
Other peers announce new blocks via `NewBlockHashes` and `NewBlock` messages, which Reth can then download and validate locally.

Reth waits for a command from the CL through Engine API method calls before advancing the canonical chain. 
This ensures that the execution layer (EL) remains synchronized with the CL decided values -- the CL is the authority on what's canonical. 

### Malachite Sync Overview

[ValueSync](https://github.com/informalsystems/malachite/tree/main/specs/synchronization) is a protocol that runs alongside consensus to help nodes catch up when they fall behind. 
It operates as a client-server system where each node runs both roles simultaneously.

**How it works**:

- **Height announcements** — Servers periodically broadcast their current consensus height to the network.
- **Gap detection** — Clients compare their local height against announced remote heights.
- **Request missing data** — When a client detects it's behind, it requests the missing height(s) from peers.
- **Serve from store** — The server retrieves the decided value and commit certificate from its local store and sends them back.
- **Deliver to consensus** — The client passes the synced data to the consensus layer, which processes it identically to data received through normal consensus operations.

When using Malachite's Channel API, ValueSync interacts with the application through two operations:

- `GetDecidedValue` — Malachite requests a previously decided value from the application's storage (used by the server to fulfill sync requests)
- `ProcessSyncedValue` — Malachite notifies the application that a value has been synced from the network (used by the client to deliver received data)

This design keeps syncing logic separate from consensus while reusing the same validation and commitment paths, i.e, a synced block goes through the same checks as a block decided in real-time.

### Emerald Sync Overview

<img src="../assets/emerald-syncing.png" width="800" />

## Sync Request Handling

The sync request contains the height, while the expected response includes the `value_bytes` and the commit `certificate`.

When the middleware (Emerald) receives the `AppMsg::GetDecidedValue` message, it processes it as follows:

1. Retrieve the earliest height from storage.
    This represents the earliest height for which the node can provide a payload (block). See the _Minimal Node Height_ section.
2. Validate the requested height range:
    - If the requested height is above the node’s current height or below the earliest available height, return `None`.
    - Otherwise, continue.
3. Retrieve the earliest unpruned height from storage.
    This is the earliest height for which the full block is available locally (no need to query Reth).
4. Fetch block data:
    - If the requested height is above the earliest unpruned height, return the decided value directly from storage.
    - Otherwise, fetch the missing block data from Reth using the Engine API method `engine_getPayloadBodiesByRange`.

To support this logic, block headers and certificates are stored for all blocks a node can provide to peers. This is necessary because Reth only stores payload bodies and does not include the metadata or consensus data required for full block reconstruction.

## Sync Response Handling

Upon receiving a response from a peer, we get the `height`, `round`, `proposer`, and `value_bytes`.

The response is processed as follows:

1. Reconstruct and validate the block using the Engine API method `engine_newPayload`.
    This validation ensures that the provided value is consistent with the execution layer’s rules before passing it back to Malachite.
2. Handle the validation responses:
    - If the execution client returns a `SYNCING` status, the node retries validation.
    - The retry mechanism resends the validation request until the execution layer returns either `VALID` or `INVALID`.
    - After each `SYNCING` response, the system waits for a configurable sleep delay before retrying.

        This was added in order to ensure proper sync in scenarios where both the consensus and execution layers are recovering from a crash.

3. Return the reconstructed proposal to Malachite once validation succeeds.

> Note:
> In the current Malachite implementation, there is no timeout during validation of syncing values.
> A configurable syncing timeout has been introduced as part of the `MalakethConfig` to address this.
>

## Minimal Node Height

The minimal node height corresponds to the certificate with the lowest available height, since certificates are stored for all supported values.

Currently, there is no direct way to determine the oldest supported height by the execution client (Reth).

Depending on the node type, behavior differs:

- Archival Node - Reth stores all blocks from genesis. The minimal height can directly reflect this.
- Full Node - Reth stores approximately the last 10,064 blocks, pruning older ones. Logic could be added so that Malachite prunes in parallel with Reth.
- Custom Node - If Reth uses a custom pruning policy (defined in `reth.toml`), the middleware would need to either:
  - Follow the same custom pruning rules, or
  - Restrict itself to providing only data available locally.

> Note:
> Emerald currently supports only archival Reth nodes.
>

## Emerald Storage overview

In order to provide a response to the `AppMsg::GetDecidedValue` message for height `h`, a node requires both the appropriate certificate and the corresponding payload. Payload bodies can be retrieved using the Engine API method `engine_getPayloadBodiesByRange`, which returns the transactions and withdrawals contained within a payload but does not include the remaining metadata. The `decided_block_headers` were therefore added to storage to support the syncing protocol.

The `block_header` type is based on `ExecutionPayloadV3`, but with transactions and withdrawals set to empty vectors (code [ref](https://github.com/informalsystems/malaketh-layered-private/blob/main/app/src/state.rs#L524C1-L543C2)).

An alternative approach would have been to use the `eth_getBlockByNumber` method and store `block_number` instead. However, since `engine_getPayloadBodiesByRange` was specifically designed for syncing purpose and allows for future batching optimizations, it was chosen instead.

## Example flow

Consider a scenario where the entire node crashes and falls behind. In this case, Reth will detect from its peers that it is lagging, and Malachite will also trigger its syncing protocol through status exchanges.

On the Malachite side, data needs to be retrieved from its host (in our setup, Emerald + Reth) to provide information to peers. When we receive the `AppMsg::GetDecidedValue` message, several situations are possible:

1. Data is available locally in Emerald - this applies only for the last few heights (5).
2. Metadata is available, but the full decided value is missing - we need to query Reth for the missing data.
3. No data is available at all.

Suppose a situation where metadata is available, but the payload bodies for the corresponding block heights must be retrieved from Reth. In this case, the decided value is reconstructed and returned to Malachite, which then forwards it to the syncing peer.

When the peer receives the decided value, it must validate it via the `engine_newPayload` API call.
If Reth is still syncing and does not yet have the required data for validation, the call will return `PayloadStatus::SYNCING`.
In that case, Malachite will retry until the operation either succeeds or times out. Once Reth returns `Valid` or `Invalid`, the peer can proceed accordingly.

A similar flow occurs when a node is joining the network where emr0 runs with reth0, and emr1 runs with reth1. A new node pair, emr2 and reth2, then intends to join the network.

For example, emr1 (Emerald 1) can provide a decided value to emr2, while reth2 queries its peers to retrieve the missing data it has fallen behind on. If emr2 (Emerald 2) reaches the newPayload call before reth2 has fully synchronized, it must wait until Reth completes syncing.
