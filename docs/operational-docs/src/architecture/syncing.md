# Syncing

## Overview

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

```mermaid
sequenceDiagram
    box Node 1 (Behind)
        participant M1 as Malachite
        participant E1 as Emerald
        participant EC1 as EL
    end
    box Node 2 (Ahead)
        participant M2 as Malachite
        participant E2 as Emerald
        participant EC2 as EL
    end

    M1->>M2: Exchange Status
    M1->>M2: requestSyncValues

    M2->>E2: AppMsg::GetDecidedValue

    alt Height available locally
        E2->>E2: Return from store
    else Height not available
        E2->>EC2: engine_getPayloadBodiesByRange
        EC2-->>E2: Payload bodies result
    end

    E2-->>M2: Return result
    M2-->>M1: Return result

    M1->>E1: AppMsg::ProcessSyncedValue
    E1->>EC1: newPayload (validation request)
    EC1-->>E1: Validation result

    alt Valid/Invalid/Accepted
        E1->>E1: Set validity accordingly
    else Syncing
        loop Retry mechanism
            E1->>EC1: newPayload (retry)
            EC1-->>E1: Validation result
        end
    end

    E1-->>M1: Return result
```

## Sync Request Handling

The sync request contains the height, while the expected response includes the `value_bytes` and the commit `certificate`.

When the application (Emerald) receives the `AppMsg::GetDecidedValue` message, it processes it as follows:

1. Retrieve from storage the _earliest available height_, i.e., the minimum height for which the application can provide a payload (block). 
   See the [Minimal Node Height](#minimal-node-height) section.
2. Validate the requested height range:
   - If the requested height is not available (i.e., below the earliest available height or above the latest decided height), return `None`.
   - Otherwise, continue.  
3. Retrieve from storage the _earliest unpruned height_, i.e., the minimum height for which the full block is available locally (no need to query the EL).
4. Fetch the block data:
    - If the requested height is below the earliest unpruned height, try fetching the missing block data from the EL using the Engine API method `engine_getPayloadBodiesByRange`. 
    - Otherwise, return the decided value directly from storage.

To support this logic, block headers and certificates are stored for all blocks a node can provide to peers. 
This is necessary because the EL only stores payload bodies and does not include the metadata or consensus data required for full block reconstruction.

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

> [!NOTE]
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

> [!NOTE]
> In order for a node to be able to sync, there has to be at least one archival node in the network that can provide historical data. We plan to add snapshot syncing to remove this constraint.
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
