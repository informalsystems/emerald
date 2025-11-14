#!/bin/bash

# Check if node ID argument is provided
if [ -z "$1" ]; then
    echo "Usage: $0 <node_id>"
    exit 1
fi

NODE=$1
NODES_HOME="nodes"
APP_BINARY="malachitebft-eth-app"

export RUST_BACKTRACE=full

echo "[Node $NODE] Restarting node..."
cargo run --bin $APP_BINARY -q -- start --home "$NODES_HOME/$NODE" --config ".testnet/config/$NODE/config.toml" > "$NODES_HOME/$NODE/logs/node.log" 2>&1 &
echo $! > "$NODES_HOME/$NODE/node.pid"
echo "[Node $NODE] Node restarted with PID $(cat $NODES_HOME/$NODE/node.pid)"
echo "[Node $NODE] Logs are available at: $NODES_HOME/$NODE/logs/node.log"
