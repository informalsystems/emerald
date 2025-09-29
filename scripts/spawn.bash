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
        --no-reset) NO_RESET=1; shift ;;
        --no-wait) NO_WAIT=1; shift ;;
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
    APP_BINARY="malachitebft-eth-app"
fi

echo "Compiling '$APP_BINARY'..."
cargo build -p $APP_BINARY

export RUST_BACKTRACE=full

# Create nodes and logs directories, run nodes
for NODE in $(seq 0 $((NODES_COUNT - 1))); do
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
    cargo run --bin $APP_BINARY -q -- start --home "$NODES_HOME/$NODE" > "$NODES_HOME/$NODE/logs/node.log" 2>&1 &
    echo $! > "$NODES_HOME/$NODE/node.pid"
    echo "[Node $NODE] Logs are available at: $NODES_HOME/$NODE/logs/node.log"
done

# Function to handle cleanup on interrupt
function exit_and_cleanup {
    echo "Stopping all nodes..."
    for NODE in $(seq 0 $((NODES_COUNT - 1))); do
        NODE_PID=$(cat "$NODES_HOME/$NODE/node.pid")
        echo "[Node $NODE] Stopping node (PID: $NODE_PID)..."
        kill "$NODE_PID"
    done
    exit 0
}

function check_reth_progress {
    NODE_PORT=$1
    INITIAL_BLOCK=$(cast block-number --rpc-url 127.0.0.1:$NODE_PORT)
    sleep 3
    NEW_BLOCK=$(cast block-number --rpc-url 127.0.0.1:$NODE_PORT)
    if [[ ! $INITIAL_BLOCK -lt $NEW_BLOCK ]]; then
        echo "No new blocks mined on node at port $NODE_PORT. Exiting with error."
        exit_and_cleanup && exit 1
    else
        echo "Node at port $NODE_PORT is making progress."
    fi
}

sleep 10

for NODE_PORT in 8545 18545 28545; do
    check_reth_progress $NODE_PORT
done

# Trap the INT signal (Ctrl+C) to run the cleanup function
trap exit_and_cleanup INT

echo "Spawned $NODES_COUNT nodes."
echo "Press Ctrl+C to stop the nodes."

if [[ -z "$NO_WAIT" ]]; then
    # Keep the script running
    sleep infinity
else
    echo "Exiting without waiting as per --no-wait flag."
    exit_and_cleanup
fi
