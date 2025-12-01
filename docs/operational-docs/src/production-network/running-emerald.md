# Running Emerald (Consensus Engine)

**Note**: This section applies to **all network participants** (both the coordinator and all validators). Each validator must run their own Emerald node with the private key they generated earlier.

Emerald is the consensus client, built on Malachite BFT. It coordinates with Reth via the Engine API to produce blocks and achieve consensus across the validator network.

## Prerequisites

- Emerald binaries installed (see [Installing Emerald](installation.md#installing-emerald))
- Node configuration directory created (contains `config.toml`, `emerald.toml`, and `priv_validator_key.json`) 
  - Recommended to setup a user `emerald` and use a home folder like `/home/emerald/.emerald` and in there a config folder for all files.
- Reth node must be running with Engine API enabled
- JWT secret file (same as used by Reth)

## Configuration Files

Each Emerald node requires two configuration files in its home directory:

**1. `config.toml` (MalachiteBFT Configuration)**

See [malachitebft-config.toml](../config-examples/malachitebft-config.toml) for a complete example. Key sections:

- **Consensus settings**: Block timing, timeouts, and consensus parameters
- **P2P networking**: Listen addresses and peer connections
  - Consensus P2P: Port `27000` (default)
    -  persistent_peers must be filled out for p2p
  - Mempool P2P: Port `28000` (default)
    -  persistent_peers must be filled out for p2p
- **Metrics**: Prometheus metrics endpoint on port `30000`

This file must be in config folder in home_dir, example `/home/emerald/.emerald/config/config.toml` where `--home` flag would be defined as `--home=/home/emerald/.emerald`

**2. `emerald.toml` (Execution Integration)**

See [emerald-config.toml](../config-examples/emerald-config.toml) for a complete example. Key settings:

```toml
moniker = "validator-0"
execution_authrpc_address = "http://<RETH_IP>:8545"
engine_authrpc_address = "http://<RETH_IP>:8551"
jwt_token_path = "/path/to/jwt.hex"
el_node_type = "archive"
sync_timeout_ms = 1000000
sync_initial_delay_ms = 100
...
```

> [!IMPORTANT]
> The `jwt_token_path` must point to the same JWT token used by Reth.

This is where you define how Emerald connects to Reth. Make sure to fill in the Reth http and authrpc address.

## Configure Peer Connections

For a multi-node network, configure persistent peers in `config.toml`:

```toml
[consensus.p2p]
listen_addr = "/ip4/0.0.0.0/tcp/27000"
persistent_peers = [
    "/ip4/<PEER1_IP>/tcp/27000",
    "/ip4/<PEER2_IP>/tcp/27000",
    "/ip4/<PEER3_IP>/tcp/27000",
]

[mempool.p2p]
listen_addr = "/ip4/0.0.0.0/tcp/28000"
persistent_peers = [
    "/ip4/<PEER1_IP>/tcp/28000",
    "/ip4/<PEER2_IP>/tcp/28000",
    "/ip4/<PEER3_IP>/tcp/28000",
]
```

Replace `<PEER_IP>` with the actual IP addresses of your validator peers.

In the Malachite BFT config.toml you will need to fill in the 2 sections (consensus.p2p and mempool.p2p) `persistent_peers` array.
It uses the format `/ip4/<IP_ADDRESS_TO_REMOTE_PEER>/tcp/<PORT_FOR_REMOTE_PEER>`. Make sure to fill in all peers in the testnet.

## Start Emerald Node

Start the Emerald consensus node:

```bash
emerald start \
  --home /home/emerald/.emerald \
  --config /home/emerald/.emerald/config/emerald.toml \
  --log-level info
```

The `--home` directory should contain:
- `<home>/config/config.toml` - Malachite BFT configuration
- `<home>/config/priv_validator_key.json` - Validator signing key
- `<home>/config/genesis.json` - Malachite BFT genesis file

An example Malachite BFT config file is provided: [malachitebft-config.toml](../config-examples/malachitebft-config.toml)

The `--config` flag should contain the explicit file path to the Emerald config:
- Example: `--config=/home/emerald/.emerald/config/emerald.toml`

## Monitoring

Emerald exposes Prometheus metrics on port 30000 (configurable in `config.toml`):

```bash
curl http://<IP>:30000/metrics
```

## Systemd Service

For production deployments, use systemd to manage the Emerald process. See [emerald.systemd.service.example](../config-examples/emerald.systemd.service.example) for a complete service configuration.

