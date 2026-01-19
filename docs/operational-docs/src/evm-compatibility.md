# EVM Compatibility

Emerald is an EVM-compatible chain. 
Developers can deploy standard EVM smart contracts and interact with the network 
using familiar Ethereum JSON-RPC APIs and tooling. 

As of the `v0.2.0` release, Emerald targets the Osaka EVM hardfork. 
While Emerald closely follows Osaka semantics, there are several important differences to be aware of. 

- **Consensus & Finality**: Emerald uses Malachite as a consensus engine and adopts a proof-of-authority (PoA) model,
  providing _sub-second deterministic finality_. 
  This contrasts with Ethereum, where finality is reached after approximately 12 minutes.
- **Block Timestamps**: Blocks are produced in under 1 second. 
  As a result, multiple consecutive blocks may share the same timestamp.
  This keeps compatibility with EVM execution engines and most block explorers that expect second-level timestamp granularity.
- **`PREV_RANDAO`**: On Ethereum, `PREV_RANDAO` can be used as a source of randomness. 
  On Emerald, `PREV_RANDAO` is always 0 and MUST NOT be used for randomness.
- **`PARENT_BEACON_BLOCK_ROOT`**: There is no beacon chain on Emerald. 
  This field is populated with the hash of the previous execution block header.
- **EIP-4844 (Blobs)**: EIP-4844 blob transactions are not supported.
- **EIP-7685 (Execution Requests)**: EIP-7685 execution request handling is not supported.