# Running Reth (Execution Client)

> [!NOTE]
> This section applies to **all network participants** (both the coordinator and all validators). 
> Each validator must run their own Reth node.

Reth is the Ethereum execution client. It handles transaction execution, state management, and provides JSON-RPC endpoints for interacting with the blockchain.

## Prerequisites

- Reth binary installed (see [Installing Reth](installation.md#installing-reth))
- Genesis file (`eth-genesis.json`) created for your network (see [Generate Genesis Files](genesis.md#step-4-generate-genesis-files)).

## Generate JWT Secret

The JWT secret is required for authenticated communication between Reth (execution client) and Emerald (consensus engine) via the Engine API.

**For the Network Coordinator**: Generate a single JWT secret and share it with all validators:

```bash
openssl rand -hex 32
```

Save this hex string to a file (e.g., `jwt.hex`) and distribute it to all validators.

> [!IMPORTANT]
> - The same JWT must be used by **both** Reth and Emerald on each node
> - **All validators** must use the **same JWT secret** (share the hex string with all participants)
> - Each node should save the JWT hex string to a file and reference it in both Reth and Emerald configurations

## Start Reth Node

Start Reth with the following configuration:

```bash
custom-reth node \
  --chain /path/to/genesis.json \
  --datadir /var/lib/reth \
  --http \
  --http.addr 0.0.0.0 \
  --http.port 8545 \
  --http.api eth,net,web3,txpool,debug \
  --http.corsdomain "*" \
  --ws \
  --ws.addr 0.0.0.0 \
  --ws.port 8546 \
  --ws.api eth,net,web3,txpool,debug \
  --authrpc.addr 0.0.0.0 \
  --authrpc.port 8551 \
  --authrpc.jwtsecret /var/lib/reth/jwt.hex \
  --port 30303 \
  --metrics=0.0.0.0:9000
```

### Key Configuration Options

- `--chain`: Path to genesis configuration file
- `--datadir`: Database and state storage directory
- `--http.*`: JSON-RPC HTTP endpoint configuration
- `--ws.*`: WebSocket endpoint configuration
- `--authrpc.*`: Authenticated Engine API for consensus client communication
- `--authrpc.jwtsecret`: Path to JWT secret file for Engine API authentication (must match Emerald's JWT)
- `--port`: P2P networking port for peer connections
- `--disable-discovery`: Disable peer discovery (useful for permissioned networks)

## Network Endpoints

Once running, Reth provides the following endpoints:

- **HTTP RPC**: `http://<IP>:8545` - Standard Ethereum JSON-RPC
- **WebSocket**: `ws://<IP>:8546` - WebSocket subscriptions
- **Engine API**: `http://<IP>:8551` - Authenticated API for Emerald consensus
- **Metrics**: `http//<IP>:9000` - Prometheus Metrics Endpoint

## Configuring Reth Peer Connections

For a multi-validator network, Reth nodes need to connect to each other to sync the blockchain state and propagate transactions. This section explains how to establish peer connections between all Reth nodes.

**Why Peering is Important:**
- Enables block and transaction propagation across the network
- Allows nodes to stay synchronized with each other
- Creates a resilient network topology

### Method 1: Using the `--trusted-peers` Flag (Recommended)

This is the recommended approach as it automatically establishes connections when nodes start up.

**Step 1: Each Validator Gets Their Enode URL**

Each validator needs to obtain their node's enode URL. The enode URL contains the node's identity and network address.

To get your enode URL, start your Reth node first (with `admin` added to `--http.api`), then run:

```bash
curl -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"admin_nodeInfo","params":[],"id":1}' \
  http://localhost:8545 | jq -r '.result.enode'
```

This will output something like:
```
enode://a0fd9e095d89320c27b2a07460f4046f63747e5b99ca14dd94475f65910bf0c67037fc1194a04d083afb13d61def3f6f1112757f514ca2fdabd566610658d030@127.0.0.1:30303
```

> [!IMPORTANT]
> Replace `127.0.0.1` with your server's **public IP address** before sharing. For example:
> ```
> enode://a0fd9e095d89320c27b2a07460f4046f63747e5b99ca14dd94475f65910bf0c67037fc1194a04d083afb13d61def3f6f1112757f514ca2fdabd566610658d030@203.0.113.10:30303
> ```

**Step 2: Network Coordinator Collects All Enode URLs**

As the network coordinator, collect the enode URLs from all validators and compile them into a single list.

**Step 3: Distribute Peer List to All Validators**

Share the complete list of enode URLs with all validators. Each validator should add the other validators' enodes (excluding their own) to their Reth startup command using the `--trusted-peers` flag:

```bash
custom-reth node \
  --chain /path/to/genesis.json \
  --datadir /var/lib/reth \
  --http \
  --http.addr 0.0.0.0 \
  --http.port 8545 \
  --http.api eth,net,web3,txpool,debug \
  --authrpc.addr 0.0.0.0 \
  --authrpc.port 8551 \
  --authrpc.jwtsecret /var/lib/reth/jwt.hex \
  --port 30303 \
  --metrics=0.0.0.0:9000 \
  --trusted-peers=enode://PEER1_ENODE@PEER1_IP:30303,enode://PEER2_ENODE@PEER2_IP:30303,enode://PEER3_ENODE@PEER3_IP:30303
```

**Example with actual values:**
```bash
--trusted-peers=enode://a0fd9e095d89320c27b2a07460f4046f63747e5b99ca14dd94475f65910bf0c67037fc1194a04d083afb13d61def3f6f1112757f514ca2fdabd566610658d030@203.0.113.10:30303,enode://add24465ccee48d97a0212afde6b2c0373c8b2b37a1f44c46be9d252896fe6c55256fd4bd8652cf5d41a11ffae1f7537922810b160a4fd3ed0c6f388d137587e@203.0.113.11:30303
```

> [!NOTE]
> - Each validator excludes their own enode from their `--trusted-peers` list
> - All enodes should use the **public IP addresses** of the validator servers
> - Make sure port 30303 (or your configured P2P port) is open in firewalls between validators

### Method 2: Adding Peers at Runtime (Alternative)

If you need to add peers to an already-running node, you can use the JSON-RPC API:

**Prerequisites:**
- Add `admin` to the `--http.api` flag when starting Reth
- This must be done on all Reth nodes that will use this method

**Add a trusted peer:**
```bash
curl -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"admin_addTrustedPeer","params":["enode://FULL_ENODE_URL"],"id":1}' \
  http://localhost:8545
```

**Example:**
```bash
curl -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"admin_addTrustedPeer","params":["enode://a0fd9e095d89320c27b2a07460f4046f63747e5b99ca14dd94475f65910bf0c67037fc1194a04d083afb13d61def3f6f1112757f514ca2fdabd566610658d030@203.0.113.10:30303"],"id":1}' \
  http://localhost:8545
```

**Verify peer connections:**
```bash
curl -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"admin_peers","params":[],"id":1}' \
  http://localhost:8545 | jq
```

**Drawback**: Peers added this way are not persisted across restarts. Use Method 1 for production deployments.

## Systemd Service

For remote deployments, you can use systemd to manage the Reth process. See [reth.systemd.service.example](../config-examples/reth.systemd.server.example) for a service configuration example.