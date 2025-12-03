# Execution Layer

Emerald integrates with Ethereum execution clients through [Engine API](https://github.com/ethereum/execution-apis/tree/main/src/engine), allowing it to plug into a mature execution ecosystem.
Currently, Emerald integrates with [Reth](https://github.com/paradigmxyz/reth) and the roadmap includes support for additional clients.

## Key Properties 

**EVM Compatibility.** Users can run existing Ethereum smart contracts on Emerald without modification. 
Consequently, they can leverage established standards such as ERC-20, ERC-721, and ERC-4626.

**Rich Ecosystem Support.** Users get immediate access to wallets, block explorers, indexers, and developer frameworks.
Emerald provides native compatibility with DeFi protocols, bridges, token standards, and interoperability layers.

**Continuous Performance Improvements.** Emerald benefits from ongoing optimizations and research from active Ethereum client development.
Users remain aligned with performance and scalability upgrades adopted by the Ethereum ecosystem.

**Reduced development overhead.** Emerald networks do not need to build or maintain a custom execution layer, resulting in faster time-to-market with lower operational burden. 

## Reth Integration 

In Emerald, it is the responsibility of the execution client to build and validate blocks as consensus has no application specific knowledge on what a block contains.
Emerald integrates with the execution client via Engine API. 

In the context of Engine API, Emerald is the consensus layer that acts as a client issuing RPC requests to the execution client, which serves as the RPC server.
The core methods are:

- `exchangeCapabilities` Allows the consensus and execution clients to exchange and negotiate supported Engine API capabilities.
  It ensures both sides understand each other’s feature set (e.g., versioned payload types, optional fields), enabling forward compatibility and coordinated upgrades across client releases.
- `forkchoiceUpdated` Updates the execution client with the Emerald's latest finalized block. 
  If `payloadAttributes` are provided, it also instructs the execution client to begin building a new block and returns a `payloadId` for later retrieval.
- `getPayload` Returns the execution payload associated with a previously issued `payloadId`. 
  It finalizes the block under construction and hands it to Emerald so it can be proposed for inclusion on-chain.
- `newPayload` Delivers a newly built (i.e., proposed) execution payload from Emerald to the execution client for validation. 
  It verifies the block’s correctness and, if valid, incorporates it into the local chain state.

For a detail description of these methods and how they work, please refer to the [Engine API visual guide](https://hackmd.io/@danielrachi/engine_api).

Emerald is calling the Engine API RPC methods when handling events emitted by the [Malachite consensus engine](consensus.md):

- `AppMsg::ConsensusReady` Emerald calls `exchangeCapabilities` to check compatibility with the execution client. 
  Then, it gets the genesis block from the execution client by calling the `eth_getBlockByNumber` Ethereum RPC. 
  Note that when restarting after a crash, Emerald retrieves the latest decided block from its local store and calls `forkchoiceUpdated` to update the head of the chain in the execution client.

- `AppMsg::StartedRound` Emerald starts the next height / round of consensus by retrieving pending proposal data and validating it against the execution client via a `newPayload` call.

- `AppMsg::GetValue` Once an Emerald node ends up being a proposer, it calls `forkchoiceUpdated` with `payloadAttributes` provided and then `getPayload` to get a new block from the execution client.
  Note that first Emerald calls the `eth_syncing` Ethereum RPC call to confirm that the execution client is not syncing.

- `AppMsg::ReceivedProposalPart` When all the parts of a proposed blocked have been received, Emerald validates the block against the execution client by calling `newPayload`.

- `AppMsg::Decided` Once consensus is reached, the block is validated again against the execution client (via a `newPayload` call).This is necessary as the proposer has not called `newPayload` in `ReceiveProposalParts`.
  Then, a call to `forkchoiceUpdated` updates the head of the chain in the execution client.
  As an optimization, Emerald avoid re-validation by caching blocks that have been validated already so that non-proposing nodes do not have to call `newPayload` twice.

- `AppMsg::ProcessSyncedValue` When Emerald is syncing, it validates blocks received from other nodes by calling `newPayload` (as in `RecieveProposalParts`).

- `AppMsg::GetDecidedValue` When other Emerald nodes are syncing, they might ask for blocks that are no longer in the local store. 
  In that case, Emerald is calling `getPayload` to get the block from the execution client. 
