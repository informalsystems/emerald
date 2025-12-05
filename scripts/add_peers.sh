#!/usr/bin/env bash
set -euo pipefail

# Script to manually add peers (their enodes) to each node
# Usage: ./scripts/add_peers.sh --nodes N

NODES_COUNT=0

while [[ "$#" -gt 0 ]]; do
    case "$1" in
        --nodes)
            NODES_COUNT="$2"
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

echo "Waiting for ${NODES_COUNT} Reth nodes to be ready and collecting enodes..."

# Wait for each node and collect its enode
for ((i = 0; i < NODES_COUNT; i++)); do
    PORT=$((PORT_BASE + i * PORT_INCREMENT))
    PORTS[i]=$PORT

    echo "Waiting for Reth node $i on port ${PORT} to be ready..."
    until cast rpc --rpc-url "127.0.0.1:${PORT}" net_listening > /dev/null 2>&1; do
        sleep 1
    done

    ENODE=$(cast rpc --rpc-url "127.0.0.1:${PORT}" admin_nodeInfo | jq -r .enode)
    ENODES[i]="$ENODE"

    echo "Node $i (port ${PORT}) ENODE: ${ENODES[i]}"
done

echo
echo "Connecting all nodes to each other..."

# For each node i, add all other nodes j as peers / trusted peers
for ((i = 0; i < NODES_COUNT; i++)); do
    RPC_URL="127.0.0.1:${PORTS[i]}"

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
