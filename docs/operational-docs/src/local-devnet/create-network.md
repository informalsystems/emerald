# Creating a Testnet

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

Within the home folder for each node there will be a `nodes/0/config/config.toml` file with Malachite specific configuration 
(see [malachitebft-config.toml](../config-examples/malachitebft-config.toml) for more details).  
To alter this configuration for more than one node, instead of opening and editing multiple files, you can use the following command:

```bash
cargo run --package emerald-utils -- modify-config --node-config-home nodes --custom-config-file-path assets/emerald_p2p_config.toml
```

replacing the `node-config-home` with the path to your testnet, `custom-config-file-path` with the path to your custom configuration. An example of the custom configuration file:

```toml
[node0]
ip = "127.0.0.1"
[node0.consensus.p2p]
listen_addr =  "/ip4/127.0.0.1/tcp/37000"
persistent_peers = [
    "/ip4/127.0.0.1/tcp/37001",
    "/ip4/127.0.0.1/tcp/27002",
]


[node1]
ip = "127.0.0.1"
[node1.consensus.p2p]
listen_addr =  "/ip4/127.0.0.1/tcp/37001"
persistent_peers = [
    "/ip4/127.0.0.1/tcp/27000",
    "/ip4/127.0.0.1/tcp/27002",
    "/ip4/127.0.0.1/tcp/27003",
]

```
The code above replaces the default `consensus.p2p` section of nodes  0 and 1 to use different ports from the default values.  

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
