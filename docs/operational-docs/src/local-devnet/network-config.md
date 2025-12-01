# Network Configuration

## Default Addresses

- **ValidatorManager Contract**: `0x0000000000000000000000000000000000002000`
- **RPC Endpoints**:
  - Node 0: `http://127.0.0.1:8645` (primary endpoint for most operations)
  - Node 1: `http://127.0.0.1:18645`
  - Node 2: `http://127.0.0.1:28645`
  - Node 3: `http://127.0.0.1:38645`

**Note**: All nodes share the same blockchain state. You can connect to any endpoint, but `8645` is typically used as the default.

## Genesis Validators

The genesis file is generated with four initial validators, each with power 100. Validator public keys are extracted from `nodes/{0,1,2,3}/config/priv_validator_key.json`.

## Pre-funded Test Accounts

The genesis file pre-funds accounts from the test mnemonic with ETH for testing:

**Mnemonic**: `test test test test test test test test test test test junk`

> TODO This list is not complete

| Account # | Address | Private Key | Initial Balance |
|-----------|---------|-------------|-----------------|
| 0 | `0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266` | `0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80` | 10,000 ETH |
| 1 | `0x70997970C51812dc3A010C7d01b50e0d17dc79C8` | `0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d` | 10,000 ETH |
| 2 | `0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC` | `0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a` | 10,000 ETH |

Use these accounts for sending transactions, deploying contracts, or testing.