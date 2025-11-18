# Testnet Init Feature Design

## Overview

Replace the Makefile-based testnet setup workflow with a comprehensive Rust CLI tool that manages complete testnet lifecycles, including both Emerald consensus nodes and Reth execution clients.

## Goals

1. **Eliminate Docker dependency** - Run Reth directly from CLI instead of docker-compose
2. **Unified management** - Single CLI tool to manage both Emerald and Reth node pairs
3. **Dynamic scaling** - Add or remove node pairs without manual configuration
4. **Process management** - Track and manage all processes with PIDs
5. **Backward compatibility** - Keep existing `testnet` command working

## Command Structure

### New Command Hierarchy

```bash
# Current command (keep for backward compatibility)
cargo run --bin malachitebft-eth-app -- testnet [OPTIONS]

# New subcommands
cargo run --bin malachitebft-eth-app -- testnet init [OPTIONS]
cargo run --bin malachitebft-eth-app -- testnet add-node [OPTIONS]
cargo run --bin malachitebft-eth-app -- testnet remove-node <NODE_ID>
cargo run --bin malachitebft-eth-app -- testnet stop [OPTIONS]
cargo run --bin malachitebft-eth-app -- testnet status
```

### Command Details

#### `testnet init`

Initialize and start a complete testnet with N node pairs.

```bash
cargo run --bin malachitebft-eth-app -- testnet init \
  --nodes 3 \
  --home ./nodes \
  [--node-keys KEY1 --node-keys KEY2 ...] \
  [--runtime single-threaded|multi-threaded[:N]] \
  [--log-level info|debug|trace]
```

**What it does:**
1. Check if `reth` is installed (`reth --version`)
2. Generate testnet configuration (reuse existing logic)
3. Extract validator public keys
4. Generate genesis.json (use existing `malachitebft-eth-utils genesis`)
5. Spawn N Reth processes with proper configuration
6. Wait for Reth nodes to reach height 1
7. Add peers between Reth nodes using `reth p2p` CLI
8. Spawn N Emerald processes
9. Monitor and report status
10. Write PIDs to `nodes/{N}/reth.pid` and `nodes/{N}/emerald.pid`

**Logs:**
- `nodes/{N}/logs/reth.log` - Reth execution client logs
- `nodes/{N}/logs/emerald.log` - Emerald consensus logs

**Config generated:**
- `.testnet/testnet_config.toml` - Testnet configuration
- `.testnet/config/{N}/config.toml` - Emerald node configs
- `nodes/{N}/config/priv_validator_key.json` - Validator keys
- `nodes/{N}/config/config.toml` - Node config
- `nodes/{N}/reth-data/` - Reth data directory
- `assets/genesis.json` - Genesis file

#### `testnet add-node`

Add a new Emerald/Reth node pair to running testnet.

```bash
cargo run --bin malachitebft-eth-app -- testnet add-node \
  [--node-key KEY] \
  [--home ./nodes]
```

**What it does:**
1. Determine next node ID (read existing nodes)
2. Generate config for new node
3. Update genesis if needed (add new validator)
4. Spawn new Reth process
5. Add new Reth to existing Reth peers
6. Add existing Reth nodes to new Reth peers
7. Spawn new Emerald process
8. Update testnet metadata

#### `testnet remove-node`

Stop and remove a node pair.

```bash
cargo run --bin malachitebft-eth-app -- testnet remove-node 3 \
  [--home ./nodes] \
  [--keep-data]
```

**What it does:**
1. Read PIDs from `nodes/3/reth.pid` and `nodes/3/emerald.pid`
2. Stop processes (SIGTERM, wait, SIGKILL if needed)
3. Remove from peer lists
4. Optionally clean up data directories
5. Update testnet metadata

#### `testnet stop`

Stop all nodes in the testnet.

```bash
cargo run --bin malachitebft-eth-app -- testnet stop \
  [--home ./nodes] \
  [--keep-data]
```

**What it does:**
1. Find all PIDs in `nodes/*/reth.pid` and `nodes/*/emerald.pid`
2. Stop all processes gracefully
3. Optionally clean up data

#### `testnet status`

Show status of all nodes.

```bash
cargo run --bin malachitebft-eth-app -- testnet status \
  [--home ./nodes]
```

**Output:**
```
Testnet Status (3 nodes):

Node 0:
  Emerald: Running (PID: 12345)
  Reth:    Running (PID: 12346)
  Height:  1234
  Peers:   2/2

Node 1:
  Emerald: Running (PID: 12347)
  Reth:    Running (PID: 12348)
  Height:  1234
  Peers:   2/2

Node 2:
  Emerald: Stopped
  Reth:    Running (PID: 12350)
  Height:  1234
  Peers:   2/2
```

## Architecture

### Module Structure

```
cli/src/cmd/
├── testnet.rs              # Main testnet command enum
└── testnet/
    ├── mod.rs              # Module exports
    ├── generate.rs         # Original testnet command (backward compat)
    ├── init.rs             # New init command
    ├── add_node.rs         # Add node command
    ├── remove_node.rs      # Remove node command
    ├── stop.rs             # Stop command
    ├── status.rs           # Status command
    ├── reth.rs             # Reth process management
    ├── emerald.rs          # Emerald process management
    ├── peers.rs            # Peer management
    └── types.rs            # Shared types
```

### Key Components

#### 1. Reth Process Manager (`reth.rs`)

```rust
pub struct RethNode {
    pub node_id: usize,
    pub home_dir: PathBuf,
    pub data_dir: PathBuf,
    pub genesis_file: PathBuf,
    pub jwt_secret: PathBuf,
    pub ports: RethPorts,
}

pub struct RethPorts {
    pub http: u16,        // 8645, 18645, 28645, ...
    pub ws: u16,          // 8646, 18646, 28646, ...
    pub authrpc: u16,     // 8551, 18551, 28551, ...
    pub metrics: u16,     // 9000, 9001, 9002, ...
    pub discovery: u16,   // 31303, 41303, 51303, ...
    pub p2p: u16,         // 31303, 41303, 51303, ...
}

impl RethNode {
    /// Check if reth is installed
    pub fn check_installation() -> Result<String> {
        // Run: reth --version
    }

    /// Build reth command arguments
    pub fn build_args(&self) -> Vec<String> {
        // Build args based on compose.yaml pattern
    }

    /// Spawn reth process
    pub fn spawn(&self) -> Result<RethProcess> {
        // Spawn process with stdout/stderr -> logs/reth.log
        // Write PID to reth.pid
    }

    /// Get enode address
    pub fn get_enode(&self) -> Result<String> {
        // Use cast rpc or reth CLI to get enode
    }

    /// Add peer
    pub fn add_peer(&self, enode: &str) -> Result<()> {
        // Use reth p2p commands or admin RPC
    }

    /// Wait for reth to reach height
    pub fn wait_for_height(&self, height: u64, timeout: Duration) -> Result<()> {
        // Poll using cast block-number
    }
}

pub struct RethProcess {
    pub pid: u32,
    pub log_file: PathBuf,
}
```

#### 2. Emerald Process Manager (`emerald.rs`)

```rust
pub struct EmeraldNode {
    pub node_id: usize,
    pub home_dir: PathBuf,
    pub config_file: PathBuf,
}

impl EmeraldNode {
    /// Spawn emerald process
    pub fn spawn(&self, log_level: &str) -> Result<EmeraldProcess> {
        // cargo run --bin malachitebft-eth-app -- start
        //   --home nodes/{N}
        //   --config .testnet/config/{N}/config.toml
        //   --log-level {level}
        // stdout/stderr -> logs/emerald.log
        // Write PID to emerald.pid
    }
}

pub struct EmeraldProcess {
    pub pid: u32,
    pub log_file: PathBuf,
}
```

#### 3. Peer Manager (`peers.rs`)

```rust
pub struct PeerManager {
    pub nodes: Vec<RethNode>,
}

impl PeerManager {
    /// Add all nodes as peers to each other
    pub fn connect_all(&self) -> Result<()> {
        // Get all enodes
        // For each node, add all other nodes as peers
    }

    /// Add new node to existing network
    pub fn add_node(&self, new_node: &RethNode) -> Result<()> {
        // Add new node to all existing nodes
        // Add all existing nodes to new node
    }

    /// Remove node from network
    pub fn remove_node(&self, node_id: usize) -> Result<()> {
        // Remove node from all peer lists
    }
}
```

#### 4. Process Manager (`types.rs`)

```rust
pub struct ProcessHandle {
    pub pid: u32,
    pub name: String,
}

impl ProcessHandle {
    /// Read PID from file
    pub fn from_pid_file(path: &Path) -> Result<Self>;

    /// Check if process is running
    pub fn is_running(&self) -> bool;

    /// Stop process (SIGTERM -> wait -> SIGKILL)
    pub fn stop(&self, timeout: Duration) -> Result<()>;

    /// Write PID to file
    pub fn write_to_file(&self, path: &Path) -> Result<()>;
}

pub struct TestnetMetadata {
    pub num_nodes: usize,
    pub created_at: SystemTime,
    pub genesis_hash: String,
}

impl TestnetMetadata {
    /// Load from nodes directory
    pub fn load(home_dir: &Path) -> Result<Self>;

    /// Save to nodes directory
    pub fn save(&self, home_dir: &Path) -> Result<()>;
}
```

## Implementation Phases

### Phase 1: Basic Structure ✅
- [ ] Restructure `TestnetCmd` to enum with subcommands
- [ ] Create module structure (`testnet/mod.rs`, etc.)
- [ ] Move existing testnet logic to `generate.rs`
- [ ] Update `args.rs` to handle new structure
- [ ] Ensure backward compatibility works

### Phase 2: Reth Management
- [ ] Implement `RethNode` struct
- [ ] Implement `check_installation()`
- [ ] Implement `build_args()` based on compose.yaml
- [ ] Implement `spawn()` with log redirection
- [ ] Test spawning single Reth node

### Phase 3: Peer Management
- [ ] Research `reth p2p` CLI commands for peer management
- [ ] Implement `get_enode()` (via admin RPC or reth CLI)
- [ ] Implement `add_peer()` using reth commands
- [ ] Implement `PeerManager::connect_all()`
- [ ] Test peer connectivity

### Phase 4: Basic Init Command
- [ ] Implement `testnet init` command structure
- [ ] Generate configs (reuse existing logic)
- [ ] Generate genesis (call existing command)
- [ ] Spawn Reth processes
- [ ] Add peers between Reth nodes
- [ ] Spawn Emerald processes
- [ ] Test complete init workflow

### Phase 5: Process Management
- [ ] Implement `ProcessHandle` for PID management
- [ ] Implement `stop` command
- [ ] Implement `status` command
- [ ] Test start/stop/status workflow

### Phase 6: Dynamic Scaling
- [ ] Implement `add-node` command
- [ ] Implement `remove-node` command
- [ ] Test adding/removing nodes from running testnet

## Reth Configuration

Based on `compose.yaml`, each Reth node needs:

### Port Allocation Pattern

```
Node N (N = 0, 1, 2, ...):
- HTTP RPC:    (N * 10000) + 8645
- WebSocket:   (N * 10000) + 8646
- AuthRPC:     (N * 10000) + 8551
- Metrics:     9000 + N
- Discovery:   ((N + 3) * 10000) + 1303
- P2P:         Same as discovery
```

### Command Line Arguments

```bash
reth node \
  -vvvvv \
  -d \
  --datadir=./nodes/{N}/reth-data \
  --chain=./assets/genesis.json \
  --http \
  --http.port={HTTP_PORT} \
  --http.addr=0.0.0.0 \
  --http.corsdomain=* \
  --http.api=admin,net,eth,web3,debug,txpool,trace,ots \
  --ws \
  --ws.port={WS_PORT} \
  --ws.addr=0.0.0.0 \
  --authrpc.addr=0.0.0.0 \
  --authrpc.port={AUTHRPC_PORT} \
  --authrpc.jwtsecret=./assets/jwtsecret \
  --metrics=127.0.0.1:{METRICS_PORT} \
  --discovery.port={DISCOVERY_PORT} \
  --port={P2P_PORT} \
  --nat=extip:127.0.0.1
```

## Peer Management with Reth

### Option 1: Using Admin RPC (Current Approach)

```bash
# Get enode
cast rpc --rpc-url 127.0.0.1:8645 admin_nodeInfo | jq -r .enode

# Add peer
cast rpc --rpc-url 127.0.0.1:8645 admin_addPeer "enode://..."
cast rpc --rpc-url 127.0.0.1:8645 admin_addTrustedPeer "enode://..."
```

### Option 2: Using Reth CLI

Research needed: Check if `reth p2p` has commands like:
```bash
reth p2p node-info --datadir=...
reth p2p add-peer --datadir=... --enode=...
```

**Decision:** Start with Admin RPC (proven to work), migrate to reth CLI if available.

## Example Usage

### Full Workflow

```bash
# 1. Initialize 3-node testnet with custom keys
cargo run --bin malachitebft-eth-app -- testnet init \
  --nodes 3 \
  --node-keys 0x1234... \
  --node-keys 0x5678... \
  --node-keys 0x9abc...

# Output:
# ✓ Reth installed: v1.9.2
# ✓ Generated testnet config
# ✓ Generated validator keys
# ✓ Generated genesis.json
# ✓ Started Reth node 0 (PID: 12345)
# ✓ Started Reth node 1 (PID: 12346)
# ✓ Started Reth node 2 (PID: 12347)
# ✓ All Reth nodes reached height 1
# ✓ Connected 3 Reth peers
# ✓ Started Emerald node 0 (PID: 12348)
# ✓ Started Emerald node 1 (PID: 12349)
# ✓ Started Emerald node 2 (PID: 12350)
#
# Testnet initialized successfully!
# Logs: nodes/{0,1,2}/logs/
# Press Ctrl+C to stop all nodes, or use: testnet stop

# 2. Check status
cargo run --bin malachitebft-eth-app -- testnet status

# 3. Add a 4th node
cargo run --bin malachitebft-eth-app -- testnet add-node

# 4. Remove node 2
cargo run --bin malachitebft-eth-app -- testnet remove-node 2

# 5. Stop all
cargo run --bin malachitebft-eth-app -- testnet stop
```

## Migration Path

### Backward Compatibility

Keep existing `testnet` command working:

```bash
# Old usage (still works)
./scripts/generate_testnet_config.sh --nodes 3 --testnet-config-dir .testnet
cargo run --bin malachitebft-eth-app -- testnet \
  --home nodes \
  --testnet-config .testnet/testnet_config.toml

# New equivalent
cargo run --bin malachitebft-eth-app -- testnet init --nodes 3
```

### Deprecation Timeline

1. **Phase 1** - Keep both commands
2. **Phase 2** - Add deprecation warning to old command
3. **Phase 3** - Remove old command (breaking change, major version bump)

## Testing Strategy

### Unit Tests

- [ ] `RethNode::build_args()` - verify correct arguments
- [ ] `RethPorts::for_node()` - verify port calculation
- [ ] `ProcessHandle::from_pid_file()` - verify PID reading
- [ ] `TestnetMetadata` serialization

### Integration Tests

- [ ] Spawn single Reth node, verify it starts
- [ ] Spawn multiple Reth nodes, verify peer connectivity
- [ ] Full `testnet init` workflow
- [ ] Add node to running testnet
- [ ] Remove node from running testnet
- [ ] Stop all nodes

### Manual Testing Checklist

- [ ] Run `testnet init --nodes 3`
- [ ] Verify all logs are created
- [ ] Verify PIDs are written
- [ ] Verify Reth nodes are connected
- [ ] Verify Emerald nodes are running
- [ ] Send transactions using spam tool
- [ ] Verify blocks are produced
- [ ] Add 4th node while running
- [ ] Remove node 2
- [ ] Stop all nodes
- [ ] Verify graceful shutdown

## Open Questions

1. **Reth peer management:** Does `reth p2p` CLI have peer management commands, or should we use admin RPC?
   - **Action:** Research reth CLI documentation
   - **Fallback:** Use admin RPC (proven to work)

2. **Genesis updates:** When adding a node, do we need to regenerate genesis or update validator set contract?
   - **Action:** Check if PoA contract allows dynamic validator updates
   - **Fallback:** Require restart of all nodes when adding validators

3. **Signal handling:** Should init command stay running and handle Ctrl+C, or just spawn and exit?
   - **Proposal:** Stay running like spawn.bash, handle Ctrl+C to stop all nodes
   - **Alternative:** Spawn and exit, require explicit `testnet stop`

4. **Monitoring:** Should we add health checks and auto-restart failed processes?
   - **Proposal:** Start simple (no auto-restart), add in future phase

## Success Criteria

- [ ] Can initialize 3-node testnet with single command
- [ ] Can use custom private keys
- [ ] All logs properly redirected to files
- [ ] Can stop all nodes with single command
- [ ] Can check status of all nodes
- [ ] Can add new node to running testnet
- [ ] Can remove node from running testnet
- [ ] No Docker dependency
- [ ] Backward compatible with existing `testnet` command
- [ ] Documentation updated (README.md)

## File Locations

```
/workspace/
├── TESTNET_INIT_DESIGN.md          # This file
├── cli/src/
│   ├── cmd/
│   │   ├── testnet.rs               # Updated: enum with subcommands
│   │   └── testnet/
│   │       ├── mod.rs               # New: module exports
│   │       ├── generate.rs          # New: old testnet logic
│   │       ├── init.rs              # New: init command
│   │       ├── add_node.rs          # New: add node
│   │       ├── remove_node.rs       # New: remove node
│   │       ├── stop.rs              # New: stop command
│   │       ├── status.rs            # New: status command
│   │       ├── reth.rs              # New: reth management
│   │       ├── emerald.rs           # New: emerald management
│   │       ├── peers.rs             # New: peer management
│   │       └── types.rs             # New: shared types
│   └── args.rs                      # Updated: handle new command enum
└── nodes/
    ├── 0/
    │   ├── config/
    │   │   ├── config.toml
    │   │   └── priv_validator_key.json
    │   ├── reth-data/               # Reth data directory
    │   ├── logs/
    │   │   ├── reth.log
    │   │   └── emerald.log
    │   ├── reth.pid
    │   └── emerald.pid
    ├── 1/ (same structure)
    └── 2/ (same structure)
```

## Next Steps

1. Review this design document
2. Get approval to proceed
3. Start with Phase 1: Basic Structure
4. Iterate through phases with testing at each step
