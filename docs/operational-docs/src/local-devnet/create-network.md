# Creating a Testnet

## Prerequisites

Before starting, ensure you have:

- [Rust toolchain](https://rust-lang.org/tools/install/) (use rustup for easiest setup)
- [Foundry](https://getfoundry.sh/introduction/installation/) (for compiling, testing, and deploying EVM smart contracts)
- [Docker](https://docs.docker.com/get-docker/)
- Docker Compose (usually included with Docker Desktop)
- Make (typically pre-installed on Linux/macOS; Windows users can use WSL)
- Git (for cloning the repository)

**Verify installations:**
```bash
rustc --version   # Should show rustc 1.85+
docker --version # Should show Docker 20.10+
make --version   # Should show GNU Make
```

## Installation

```bash
git clone https://github.com/informalsystems/emerald.git
cd emerald
make build
``` 

> [!NOTE]
> For building in release mode, use `make release`.

## Start the Network

The default configuration creates a four validator network. From the repository root, run:

```bash
make testnet-start
```

The command performs all setup automatically. See [below](#step-by-step) for a step by step deployment.

### Verify Network is Running

Once the command completes, verify the network is operational:

```bash
# Check if blocks are being produced
curl -X POST http://127.0.0.1:8645 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'

# Expected output: {"jsonrpc":"2.0","id":1,"result":"0x5"} (or higher block number)
```

**Access monitoring tools:**
- **Grafana Dashboard**: http://localhost:4000 (metrics visualization)
- **Prometheus**: http://localhost:9090 (raw metrics data)
- **Otterscan Block Explorer**: http://localhost:80 (view blocks and transactions)

### Step by Step

1. **Testnet Data Cleanup**
  
    ```bash
    make testnet-clean
    ```

2. **Emerald Compilation**
  
    ```bash
    make release
    ``` 

3. **Configuration Generation** 
  
    ```bash
    ./scripts/generate_testnet_config.sh --nodes 4 --testnet-config-dir .testnet
    ```
    
    - Creates `.testnet/testnet_config.toml` with network parameters

4. **Validator Key Generation**
  
    ```bash
    cargo run --bin emerald -- testnet \
      --home nodes \
      --testnet-config .testnet/testnet_config.toml
    ```
    
    - Create `nodes/0/`, `nodes/1/`, etc.
    - Every node gets a `config/priv_validator_key.json`

5. **Public Key Extraction**
  
    ```bash
    # run for every node
    cargo run --bin emerald show-pubkey \
      nodes/0/config/priv_validator_key.json
    ```
    
    - Outputs public keys to `nodes/validator_public_keys.txt`

6. **Genesis File Generation**
  
    ```bash
    cargo run --bin emerald-utils genesis \
      --public-keys-file ./nodes/validator_public_keys.txt \
      --devnet
    ```
    
    - Creates `assets/genesis.json` with:
      - Initial validator set (four validators with power 100 each)
      - ValidatorManager contract deployed at genesis
      - Ethereum genesis block configuration

7. **Reth & Monitoring Startup**
  
    ```bash
    docker compose up -d reth0 reth1 reth2 reth3 prometheus grafana otterscan
    ```
    
    - Docker Compose starts Reth execution clients and monitoring services
    - Each Reth node initializes from `assets/genesis.json`

8. **Reth Peer Connection**
 
    ```bash
    ./scripts/add_peers.sh --nodes 4
    ```

9.  **Emerald Startup**  
  
    ```bash
    bash scripts/spawn.bash --nodes 4 --home nodes --no-delay
    ```

## Restart a Node

Use the following command to stop the node with ID `1` (folder `nodes/1`): 

```bash
make testnet-node-stop NODE=1 
```

Then use the following command to restart a stopped node:

```bash
make testnet-node-restart NODE=1 
```

Note that without providing the node ID, the commands default to node 0. 

## Stop the Network

```bash
make testnet-stop
```

This stops all Docker containers but preserves data.

## Clean the Network

```bash
make testnet-clean
```

**Warning**: All testnet data is deleted.

- All node data (`nodes/`)
- Genesis file (`assets/genesis.json`)
- Testnet config (`.testnet/`)
- Docker volumes (Reth databases)
- Prometheus/Grafana data
