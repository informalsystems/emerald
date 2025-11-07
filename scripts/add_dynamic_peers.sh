#!/usr/bin/env bash

# Script to manually add peers (their enodes) to each node

NODES_COUNT=0
while [[ "$#" -gt 0 ]]; do
    case $1 in
        --nodes) NODES_COUNT="$2"; shift ;;
        *) echo "Unknown parameter: $1"; exit 1 ;;
    esac
    shift
done

LIMIT=0
PORT=8545
for i in $(seq 0 $((NODES_COUNT - 1))); do
    echo "Waiting for Reth node ${i} on port $((PORT + i)) to be ready..."
    until cast rpc --rpc-url "127.0.0.1:$((PORT + i))" net_listening > /dev/null 2>&1; do
        echo "Reth node ${i} not ready yet. Retrying attempt $LIMIT of 10..."
        sleep 1
        LIMIT=$((LIMIT + 1))
        if [ $LIMIT -gt 10 ]; then
            echo "Timeout waiting for Reth node ${i} to be ready."
            exit 1
        fi
    done
done

RETH_ENODES=()
for i in $(seq 0 $((NODES_COUNT - 1))); do
    ENODE=$(cast rpc --rpc-url 127.0.0.1:$((PORT + i)) admin_nodeInfo | jq -r .enode )
    RETH_ENODES+=("$ENODE")
done

for i in $(seq 0 $((NODES_COUNT - 1))); do
    cast rpc --rpc-url 127.0.0.1:$((PORT + i)) admin_addTrustedPeer "${RETH_ENODES[i]}"
    cast rpc --rpc-url 127.0.0.1:$((PORT + i)) admin_addPeer "${RETH_ENODES[i]}"
done
