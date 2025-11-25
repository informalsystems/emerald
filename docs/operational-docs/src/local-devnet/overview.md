# What is a Local Devnet?

A local devnet is a fully functional blockchain network running entirely on your computer. It provides:

- **Fast iteration**: Test smart contracts and applications without waiting for public networks
- **Complete control**: Add/remove validators, modify network parameters, reset state anytime
- **No cost**: No real tokens required for testing
- **Privacy**: All transactions and data stay on your machine

## Use Cases

- Developing and testing smart contracts
- Testing validator operations and network behavior
- Experimenting with PoA validator management
- Integration testing for dApps
- Learning how Emerald consensus works

## Architecture

Emerald's architecture is intentionally clean and composable, consisting of four key components:

- The [Malachite](https://github.com/circlefin/malachite) consensus engine (instant finality)
- An Ethereum execution client (currently [Reth](https://github.com/paradigmxyz/reth))
- A lightweight shim layer that connects consensus and execution via [Engine API](https://github.com/ethereum/execution-apis/tree/main/src/engine) with JWT authentication
- A proof-of-authority (PoA) smart contract deployed at `0x0000000000000000000000000000000000002000`

The devnet setup creates multiple validator nodes that reach consensus on blocks with instant finality.

## Difference from Production Networks

| Feature | Local Devnet | Production Network |
|---------|---------------|-------------------|
| Validators | All on your machine | Distributed across organizations |
| Data persistence | Can reset anytime | Permanent blockchain history |
| Network access | Localhost only | Public or permissioned network |
| Use case | Development/testing | Real applications |
| Setup time | ~30 seconds | Requires coordination |