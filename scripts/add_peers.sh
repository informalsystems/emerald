#!/usr/bin/env bash
set -euo pipefail

# Script to manually add peers (their enodes) to each node
# Usage: ./scripts/add_peers.sh --nodes N [--config PATH_TO_TOML]

NODES_COUNT=0
CONFIG_FILE=""

while [[ "$#" -gt 0 ]]; do
    case "$1" in
        --nodes)
            NODES_COUNT="$2"
            shift 2
            ;;
        --config)
            CONFIG_FILE="$2"
            shift 2
            ;;
        *)
            echo "Unknown parameter: $1" >&2
            exit 1
            ;;
    esac
done

if [[ "$NODES_COUNT" -le 0 ]]; then
    echo "You must specify a positive --nodes value" >&2
    exit 1
fi

PORT_BASE=8645
PORT_INCREMENT=100

declare -a PORTS
declare -a ENODES
declare -a NODE_IPS

# Parse TOML config file if provided
if [[ -n "$CONFIG_FILE" ]]; then
    if [[ ! -f "$CONFIG_FILE" ]]; then
        echo "Config file not found: $CONFIG_FILE" >&2
        exit 1
    fi

    echo "Parsing config file: $CONFIG_FILE"

    # Extract IPs for each node from the TOML file
    for ((i = 0; i < NODES_COUNT; i++)); do
        # Look for [nodeN] section and extract the ip field
        IP=$(awk -v node="node$i" '
            $0 ~ "^\\[" node "\\]" { found=1; next }
            found && /^ip = / {
                gsub(/^ip = "/, "")
                gsub(/".*$/, "")
                print
                exit
            }
            /^\[/ && found { exit }
        ' "$CONFIG_FILE")

        if [[ -n "$IP" ]]; then
            NODE_IPS[i]="$IP"
            echo "Node $i IP: ${NODE_IPS[i]}"
        else
            NODE_IPS[i]="127.0.0.1"
            echo "Node $i IP not found in config, using default: 127.0.0.1"
        fi
    done
else
    # No config file provided, use default localhost for all nodes
    for ((i = 0; i < NODES_COUNT; i++)); do
        NODE_IPS[i]="127.0.0.1"
    done
fi

echo "Waiting for ${NODES_COUNT} Reth nodes to be ready and collecting enodes..."

# Wait for each node and collect its enode
for ((i = 0; i < NODES_COUNT; i++)); do
    PORT=$((PORT_BASE + i * PORT_INCREMENT))
    PORTS[i]=$PORT
    NODE_IP="${NODE_IPS[i]}"

    echo "Waiting for Reth node $i on ${NODE_IP}:${PORT} to be ready..."
    until cast rpc --rpc-url "${NODE_IP}:${PORT}" net_listening > /dev/null 2>&1; do
        sleep .1 # 100ms
    done

    ENODE=$(cast rpc --rpc-url "${NODE_IP}:${PORT}" admin_nodeInfo | jq -r .enode)
    ENODES[i]="$ENODE"

    echo "Node $i (${NODE_IP}:${PORT}) ENODE: ${ENODES[i]}"
done

echo
echo "Connecting all nodes to each other..."

# For each node i, add all other nodes j as peers / trusted peers
for ((i = 0; i < NODES_COUNT; i++)); do
    RPC_URL="${NODE_IPS[i]}:${PORTS[i]}"

    for ((j = 0; j < NODES_COUNT; j++)); do
        # Don't add self as peer
        if [[ "$i" -eq "$j" ]]; then
            continue
        fi

        ENODE="${ENODES[j]}"

        echo "Adding node $j as peer to node $i (rpc ${RPC_URL})"
        cast rpc --rpc-url "${RPC_URL}" admin_addTrustedPeer "${ENODE}"
        cast rpc --rpc-url "${RPC_URL}" admin_addPeer "${ENODE}"
    done
done

echo "Done: fully meshed ${NODES_COUNT} nodes."
