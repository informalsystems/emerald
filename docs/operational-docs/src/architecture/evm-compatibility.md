# EVM Compatibility


Emerald is an EVM compatible chain, allowing developers to deploy smart contracts and query the RPC endpoints of standard EVM execution environments. 

Emerald, as of the latest release (0.2.0) targetting the Osaka EVM hardfork. Compared to Osaka there are a few notable differences:


- **Finality** : Emerald provides *instant* finality achievable under 1s vs Etheriums finaluty of ~12min.
- **Consensus guarantees** : Emerald provides a Smart Contract that supports PoA (Proof of Authority) vs PoS (Proof of State) in etherium. 
- **Block timestamo**: Blocks are generated in less than 1s and it can happen that multiple bloks have the same timestamp (as Etherium execution engine and block explorers use second for block timestamp granularity.)
- `PREV_RANDAO` - On Etherium this can be used as source of randomness, it is always `0` in Emerald. Please do not use this as source of randomness. 
- **EIP-4844 blobs** - Currently not supported
- **Tokenomics** - There is currently no native token on Emerald. 
- `PARENT_BEACON_BLOCK_ROOT` - No beacon block at the moment. This is the hash of the last block from the execution header. 
- `IP7685` execution request handling is not supported. 