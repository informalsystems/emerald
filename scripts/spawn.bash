#!/usr/bin/env bash

# This script takes:
# - a number of nodes to run as an argument,
# - the home directory for the nodes configuration folders

function help {
    echo "Usage: spawn.sh [--help] --nodes NODES_COUNT --home NODES_HOME [--app APP_BINARY] [--no-reset] [--no-wait]"
}

# Parse arguments
while [[ "$#" -gt 0 ]]; do
    case $1 in
        --help) help; exit 0 ;;
        --nodes) NODES_COUNT="$2"; shift ;;
        --home) NODES_HOME="$2"; shift ;;
        --app) APP_BINARY="$2"; shift ;;
        --no-reset) NO_RESET=1 ;;
        --no-delay) NO_DELAY=1 ;;
        --no-wait) NO_WAIT=1 ;;
        *) echo "Unknown parameter passed: $1"; help; exit 1 ;;
    esac
    shift
done

# Check required arguments
if [[ -z "$NODES_COUNT" ]]; then
    help
    exit 1
fi

if [[ -z "$NODES_HOME" ]]; then
    help
    exit 1
fi

if [[ -z "$APP_BINARY" ]]; then
    APP_BINARY="emerald"
fi

echo "Compiling '$APP_BINARY'..."
cargo build --release -p $APP_BINARY

export RUST_BACKTRACE=full

# Function to handle cleanup on interrupt
function exit_and_cleanup {
    RETURN_CODE=$1
    echo "Stopping all nodes..."
    for NODE in $(seq 0 $((NODES_COUNT - 1))); do
        NODE_PID=$(cat "$NODES_HOME/$NODE/node.pid")
        echo "[Node $NODE] Stopping node (PID: $NODE_PID)..."
        kill "$NODE_PID"
    done
    if [[ -z "$RETURN_CODE" ]]; then
        exit 0
    else
        exit $RETURN_CODE
    fi
}

function wait_for_reth {
    NODE_PORT=$1
    echo "Waiting for reth node at port $NODE_PORT to reach height 1..."
    echo "trying 20 times"
    for i in $(seq 1 20); do
        BLOCK_NUMBER=$(cast block-number --rpc-url 127.0.0.1:$NODE_PORT)
        if [[ $BLOCK_NUMBER -ge 1 ]]; then
            echo "Reth node at port $NODE_PORT has reached height $BLOCK_NUMBER."
            return
        else
            echo "Current block number: $BLOCK_NUMBER. Waiting..."
            sleep 3
        fi
    done
    echo "Reth node at port $NODE_PORT did not reach height 1 in time. Exiting with error."
    exit_and_cleanup 1
}

function check_reth_progress {
    NODE_PORT=$1
    INITIAL_BLOCK=$(cast block-number --rpc-url 127.0.0.1:$NODE_PORT)
    sleep 5
    NEW_BLOCK=$(cast block-number --rpc-url 127.0.0.1:$NODE_PORT)
    if [[ ! $INITIAL_BLOCK -lt $NEW_BLOCK ]]; then
        echo "No new blocks mined on node at port $NODE_PORT. Exiting with error."
        exit_and_cleanup 1
    else
        echo "Node at port $NODE_PORT is making progress."
    fi
}

# Function to spawn a node
function spawn_node {
    NODE=$1
    if [[ -z "$NO_RESET" ]]; then
        echo "[Node $NODE] Resetting the database..."
        rm -rf "$NODES_HOME/$NODE/db"
        mkdir -p "$NODES_HOME/$NODE/db"
        rm -rf "$NODES_HOME/$NODE/wal"
        mkdir -p "$NODES_HOME/$NODE/wal"
    fi
    rm -rf "$NODES_HOME/$NODE/logs"
    mkdir -p "$NODES_HOME/$NODE/logs"
    rm -rf "$NODES_HOME/$NODE/traces"
    mkdir -p "$NODES_HOME/$NODE/traces"
    echo "[Node $NODE] Spawning node..."
    cargo run --bin $APP_BINARY -q -- start --home "$NODES_HOME/$NODE" --log-level debug --config ".testnet/config/$NODE"/config.toml > "$NODES_HOME/$NODE/logs/node.log" 2>&1 &
    echo $! > "$NODES_HOME/$NODE/node.pid"
    echo "[Node $NODE] Logs are available at: $NODES_HOME/$NODE/logs/node.log"
}

# Spawn nodes based on delay setting
if [[ -z "$NO_DELAY" ]]; then
    # Spawn all nodes except the last one
    for NODE in $(seq 0 $((NODES_COUNT - 2))); do
        spawn_node $NODE
    done
    
    # Wait for first node to reach height 10
    NODE=$((NODES_COUNT - 1))
    echo "[Node $NODE] Waiting for first node (port 8645) to reach height 100 before starting the last node..."
    for i in $(seq 1 100); do
        BLOCK_NUMBER=$(cast block-number --rpc-url 127.0.0.1:8645 2>/dev/null || echo "0")
        if [[ $BLOCK_NUMBER -ge 100 ]]; then
            echo "First node has reached height $BLOCK_NUMBER."
            break
        else
            echo "Current block number: $BLOCK_NUMBER. Waiting... (attempt $i/100)"
            sleep 2
        fi
    done
    echo "[Node $NODE] 🛸 Starting the last node..."
    
    # Spawn the last node
    spawn_node $NODE
else
    # Spawn all nodes at once
    for NODE in $(seq 0 $((NODES_COUNT - 1))); do
        spawn_node $NODE
    done
fi

wait_for_reth 8645

for ((i = 0; i < NODES_COUNT; i++)); do
    PORT=$((8645 + i * 100))
    check_reth_progress $PORT || exit_and_cleanup 1
done

# Trap the INT signal (Ctrl+C) to run the cleanup function
trap exit_and_cleanup INT

echo "Spawned $NODES_COUNT nodes."
echo "Press Ctrl+C to stop the nodes."

if [[ -z "$NO_WAIT" ]]; then
    # Keep the script running
    tail -f /dev/null
else
    echo "Exiting without waiting as per --no-wait flag."
    exit_and_cleanup
fi
