# Creating a Devnet

## Prerequisites

Before starting, ensure you have:

- **Rust**: Install from https://rust-lang.org/tools/install/
- **Docker**: Install from https://docs.docker.com/get-docker/
- **Docker Compose**: Usually included with Docker Desktop
- **Make**: Typically pre-installed on Linux/macOS; Windows users can use WSL
- **Git**: For cloning the repository

**Verify installations:**
```bash
rust --version   # Should show rustc 1.70+
docker --version # Should show Docker 20.10+
make --version   # Should show GNU Make
```

## Quick Start: 3-Validator Network

The default configuration creates a 3-validator network. From the repository root, run:

```bash
make
```

This single command performs all setup automatically:

1. **Cleans previous devnet data** - Removes any old network state
2. **Builds the project** - Compiles Solidity contracts and Rust binaries
3. **Generates devnet configuration** - Creates network parameters for 3 nodes
4. **Creates validator keys** - Generates private keys for each validator
5. **Creates node directories** - Sets up `nodes/0/`, `nodes/1/`, `nodes/2/`
6. **Extracts validator public keys** - Collects pubkeys into `nodes/validator_public_keys.txt`
7. **Generates genesis file** - Creates `assets/genesis.json` with initial validator set
8. **Starts Docker containers** - Launches Reth nodes, Prometheus, Grafana, Otterscan
9. **Configures peer connections** - Connects all Reth nodes to each other
10. **Spawns Emerald consensus nodes** - Starts the consensus layer for each validator

**Expected output:** You should see logs from all 3 validators producing blocks. The network should start producing blocks within a few seconds.

## Verify Network is Running

Once the command completes, verify the network is operational:

```bash
# Check if blocks are being produced
curl -X POST http://127.0.0.1:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'

# Expected output: {"jsonrpc":"2.0","id":1,"result":"0x5"} (or higher block number)
```

**Access monitoring tools:**
- **Grafana Dashboard**: http://localhost:3000 (metrics visualization)
- **Prometheus**: http://localhost:9090 (raw metrics data)
- **Otterscan Block Explorer**: http://localhost:5100 (view blocks and transactions)

## 4-Validator Network

To create a network with 4 validators:

```bash
make four
```

## Step by Step

1. **Configuration Generation** (`./scripts/generate_testnet_config.sh`)
   - Creates `.testnet/testnet_config.toml` with network parameters

2. **Validator Key Generation**

   ```bash
   cargo run --bin emerald -- testnet \
     --home nodes \
     --testnet-config .testnet/testnet_config.toml
   ```

   - Creates `nodes/0/`, `nodes/1/`, etc.
   - Each node gets a `config/priv_validator_key.json`

3. **Public Key Extraction**

   ```bash
   cargo run --bin emerald show-pubkey \
     nodes/0/config/priv_validator_key.json
   ```

   - Outputs public keys to `nodes/validator_public_keys.txt`

4. **Genesis File Generation**

   ```bash
   cargo run --bin emerald-utils genesis \
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
   - Emerald consensus nodes spawn and connect to Reth via Engine API