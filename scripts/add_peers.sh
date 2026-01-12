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

PORT=8645
PORT_INCREMENT=10000

for _ in {0..NODES_COUNT-1}; do
    echo "Waiting for Reth node on port ${PORT} to be ready..."
    until cast rpc --rpc-url "127.0.0.1:${PORT}" net_listening > /dev/null 2>&1; do
        sleep .1 # 100ms
    done
    PORT=$((PORT + PORT_INCREMENT))
done

RETH0_ENODE=$(cast rpc --rpc-url 127.0.0.1:8645 admin_nodeInfo | jq -r .enode )
RETH1_ENODE=$(cast rpc --rpc-url 127.0.0.1:18645 admin_nodeInfo | jq -r .enode )
RETH2_ENODE=$(cast rpc --rpc-url 127.0.0.1:28645 admin_nodeInfo | jq -r .enode )

echo "RETH0_ENODE: ${RETH0_ENODE}"
cast rpc --rpc-url 127.0.0.1:8645 admin_addTrustedPeer "${RETH1_ENODE}"
cast rpc --rpc-url 127.0.0.1:8645 admin_addTrustedPeer "${RETH2_ENODE}"
cast rpc --rpc-url 127.0.0.1:8645 admin_addPeer "${RETH1_ENODE}"
cast rpc --rpc-url 127.0.0.1:8645 admin_addPeer "${RETH2_ENODE}"

echo "RETH1_ENODE: ${RETH1_ENODE}"
cast rpc --rpc-url 127.0.0.1:18645 admin_addTrustedPeer "${RETH0_ENODE}"
cast rpc --rpc-url 127.0.0.1:18645 admin_addTrustedPeer "${RETH2_ENODE}"
cast rpc --rpc-url 127.0.0.1:18645 admin_addPeer "${RETH0_ENODE}"
cast rpc --rpc-url 127.0.0.1:18645 admin_addPeer "${RETH2_ENODE}"

echo "RETH2_ENODE: ${RETH2_ENODE}"
cast rpc --rpc-url 127.0.0.1:28645 admin_addTrustedPeer "${RETH0_ENODE}"
cast rpc --rpc-url 127.0.0.1:28645 admin_addTrustedPeer "${RETH1_ENODE}"
cast rpc --rpc-url 127.0.0.1:28645 admin_addPeer "${RETH0_ENODE}"
cast rpc --rpc-url 127.0.0.1:28645 admin_addPeer "${RETH1_ENODE}"

# If 4 nodes, add reth3
if [ "$NODES_COUNT" -eq 4 ]; then
    RETH3_ENODE=$(cast rpc --rpc-url 127.0.0.1:38645 admin_nodeInfo | jq -r .enode )

    echo "RETH3_ENODE: ${RETH3_ENODE}"

    # Add reth3 to all other nodes
    cast rpc --rpc-url 127.0.0.1:8645 admin_addTrustedPeer "${RETH3_ENODE}"
    cast rpc --rpc-url 127.0.0.1:8645 admin_addPeer "${RETH3_ENODE}"

    cast rpc --rpc-url 127.0.0.1:18645 admin_addTrustedPeer "${RETH3_ENODE}"
    cast rpc --rpc-url 127.0.0.1:18645 admin_addPeer "${RETH3_ENODE}"

    cast rpc --rpc-url 127.0.0.1:28645 admin_addTrustedPeer "${RETH3_ENODE}"
    cast rpc --rpc-url 127.0.0.1:28645 admin_addPeer "${RETH3_ENODE}"

    # Add all nodes to reth3
    cast rpc --rpc-url 127.0.0.1:38645 admin_addTrustedPeer "${RETH0_ENODE}"
    cast rpc --rpc-url 127.0.0.1:38645 admin_addTrustedPeer "${RETH1_ENODE}"
    cast rpc --rpc-url 127.0.0.1:38645 admin_addTrustedPeer "${RETH2_ENODE}"
    cast rpc --rpc-url 127.0.0.1:38645 admin_addPeer "${RETH0_ENODE}"
    cast rpc --rpc-url 127.0.0.1:38645 admin_addPeer "${RETH1_ENODE}"
    cast rpc --rpc-url 127.0.0.1:38645 admin_addPeer "${RETH2_ENODE}"
fi
