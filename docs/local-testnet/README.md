# Testnet Setup Guide

This guide explains how to create and manage a local Emerald testnet using the Makefile and the Proof-of-Authority (PoA) utilities.

## Table of Contents

- [Overview](#overview)
- [Creating a New Network](#creating-a-new-network)
- [Managing Validators with PoA Tools](#managing-validators-with-poa-tools)
- [Network Configuration](#network-configuration)
- [Troubleshooting](#troubleshooting)

## Overview

Emerald uses Malachite BFT consensus connected to Reth execution clients via Engine API. The testnet setup creates multiple validator nodes that reach consensus on blocks with instant finality.

### Architecture

- **Consensus Layer**: Malachite BFT (instant finality)
- **Execution Layer**: Reth (Ethereum execution client)
- **Connection**: Engine API with JWT authentication
- **Validator Management**: ValidatorManager PoA smart contract at `0x0000000000000000000000000000000000002000`

## Creating a New Network

### Quick Start: 3-Validator Network

The default configuration creates a 3-validator network:

```bash
make
```

This command performs the following steps:

1. Cleans previous testnet data
2. Builds the project (Solidity contracts + Rust binaries)
3. Generates testnet configuration for 3 nodes
4. Creates validator keys and node directories
5. Extracts validator public keys
6. Generates genesis file with initial validators
7. Starts Docker containers (Reth nodes, Prometheus, Grafana, Otterscan)
8. Configures peer connections
9. Spawns Malachite consensus nodes

**Monitoring**: Grafana dashboard available at http://localhost:3000

### 4-Validator Network

To create a network with 4 validators:

```bash
make four
```

### What Happens During Network Creation

1. **Configuration Generation** (`./scripts/generate_testnet_config.sh`)
   - Creates `.testnet/testnet_config.toml` with network parameters

2. **Validator Key Generation**

   ```bash
   cargo run --bin malachitebft-eth-app -- testnet \
     --home nodes \
     --testnet-config .testnet/testnet_config.toml
   ```

   - Creates `nodes/0/`, `nodes/1/`, etc.
   - Each node gets a `config/priv_validator_key.json`

3. **Public Key Extraction**

   ```bash
   cargo run --bin malachitebft-eth-app show-pubkey \
     nodes/0/config/priv_validator_key.json
   ```

   - Outputs public keys to `nodes/validator_public_keys.txt`

4. **Genesis File Generation**

   ```bash
   cargo run --bin malachitebft-eth-utils genesis \
     --public-keys-file ./nodes/validator_public_keys.txt
   ```

   - Creates `assets/genesis.json` with:
     - Initial validator set (3 validators with power 100 each)
     - ValidatorManager contract deployed at genesis
     - Ethereum genesis block configuration

5. **Network Startup**
   - Docker Compose starts Reth execution clients
   - Each Reth node initializes from `assets/genesis.json`
   - Peer connections established
   - Malachite consensus nodes spawn and connect to Reth via Engine API

## Managing Validators with PoA Tools

Once the network is running, you can manage validators using the Rust-based PoA utilities.

### Prerequisites

- Network must be running (`make` or `make four`)
- RPC endpoint accessible (default: `http://127.0.0.1:8545`)
- Contract owner private key (mnemonic "test test test test test test test test test test test junk")

**notes: **

- `make` creates testnet validator keys under relative path `nodes/{0,1,2}/config/priv_validator_key.json`.
- for local testnet the PoA contract owner private key is `0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80` which corresponds to signer index 0 of the menomnic shown above.

### List Current Validators

View all registered validators and their voting power:

```bash
cargo run --bin malachitebft-eth-utils poa list
```

**Output:**

```
Total validators: 3

Validator #1:
  Power: 100
  Pubkey: 04681eaaa34e491e6c8335abc9ea92b024ef52eb91442ca3b84598c79a79f31b75...
  Validator address: 0x1234567890abcdef...

Validator #2:
  Power: 100
  ...
```

### Add a New Validator

To add a new validator to the active set:

First get the pubkey of the validator you want to add by running:

```bash
cargo run --bin malachitebft-eth-app show-pubkey \
  path/to/new/validator/priv_validator_key.json
```

Then run the following command, replacing the placeholder values:

```bash
cargo run --bin malachitebft-eth-utils poa add-validator \
  --validator-pubkey 0x04abcdef1234567890... \
  --power 100 \
  --owner-private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```

**Parameters:**

- `--validator-pubkey`: Uncompressed secp256k1 public key (65 bytes with `0x04` prefix, or 64 bytes raw)
- `--power`: Voting weight (default: 100)
- `--owner-private-key`: Private key of the ValidatorManager contract owner

**Optional flags:**

- `--rpc-url`: RPC endpoint (default: `http://127.0.0.1:8545`)
- `--contract-address`: ValidatorManager address (default: `0x0000000000000000000000000000000000002000`)

### Remove a Validator

To remove a validator from the active set:

```bash
cargo run --bin malachitebft-eth-utils poa remove-validator \
  --validator-pubkey 0x04681eaaa34e491e6c8335abc9ea92b024ef52eb91442ca3b84598c79a79f31b75... \
  --owner-private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```

### Update Validator Power

To change a validator's voting weight:

```bash
cargo run --bin malachitebft-eth-utils poa update-validator \
  --validator-pubkey 0x04681eaaa34e491e6c8335abc9ea92b024ef52eb91442ca3b84598c79a79f31b75... \
  --power 200 \
  --owner-private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```

## Network Configuration

### Default Addresses

- **ValidatorManager Contract**: `0x0000000000000000000000000000000000002000`
- **RPC Endpoints**:
  - Node 0: `http://127.0.0.1:8545`
  - Node 1: `http://127.0.0.1:8546`
  - Node 2: `http://127.0.0.1:8547`
  - Node 3 (if running): `http://127.0.0.1:8548`

### Genesis Validators

The genesis file is generated with 3 initial validators, each with power 100. Validator public keys are extracted from:

- `nodes/0/config/priv_validator_key.json`
- `nodes/1/config/priv_validator_key.json`
- `nodes/2/config/priv_validator_key.json`

## Network Operations

### Stop the Network

```bash
make stop
```

This stops all Docker containers but preserves data.

### Clean the Network

```bash
make clean
```

**Warning**: This deletes:

- All node data (`nodes/`)
- Genesis file (`assets/genesis.json`)
- Testnet config (`.testnet/`)
- Docker volumes (Reth databases)
- Prometheus/Grafana data

### Restart a Clean Network

```bash
make clean
make
```

## Troubleshooting

### Network Won't Start

1. **Check if ports are in use**:

   ```bash
   lsof -i :8545  # RPC port
   lsof -i :30303 # P2P port
   ```

2. **View Docker logs**:

   ```bash
   docker compose logs reth0
   docker compose logs reth1
   ```

3. **Verify genesis file exists**:

   ```bash
   ls -la assets/genesis.json
   ```

4. **Check emerald logs**:
   ```bash
   tail -f nodes/0/emerald.log
   ```

### Validator Operations Fail

1. **Verify network is running**:

   ```bash
   curl -X POST http://127.0.0.1:8545 \
     -H "Content-Type: application/json" \
     -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
   ```

2. **Check validator public key format**:
   - Must be hex-encoded secp256k1 public key
   - Can be 64 bytes (raw) or 65 bytes (with `0x04` prefix)
   - Include `0x` prefix

3. **Verify contract owner key**:
   - Default: `0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80`

### Public Key Extraction

To get a validator's public key from their private key file:

```bash
cargo run --bin malachitebft-eth-app show-pubkey \
  nodes/0/config/priv_validator_key.json
```

## Monitoring

The `make` command starts monitoring services:

- **Grafana**: http://localhost:3000 (metrics dashboards)
- **Prometheus**: http://localhost:9090 (raw metrics)
- **Otterscan**: http://localhost:5100 (block explorer)

## References

- [Main README](../../README.md) - Project overview and architecture
- [Makefile](../../Makefile) - Build and deployment automation
- [ValidatorManager.sol](../../solidity/src/ValidatorManager.sol) - Validator registry contract
- [utils/src/poa.rs](../../utils/src/poa.rs) - PoA management utilities
