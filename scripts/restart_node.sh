#!/bin/bash

EXEC_ENGINE="reth"
COMPOSE_FILE="compose.yaml"
# Check if node ID argument is provided
if [ -z "$1" ]; then
    echo "Usage: $0 <node_id>"
    exit 1
fi

if [ "$2" ]; then
EXEC_ENGINE=$2
COMPOSE_FILE="compose_$EXEC_ENGINE.yaml"
fi

echo "execution engine set to $EXEC_ENGINE, using compose file $COMPOSE_FILE" 

NODE=$1
NODES_HOME="nodes"
APP_BINARY="emerald"


echo "Starting  $EXEC_ENGINE node"
docker compose -f $COMPOSE_FILE start $EXEC_ENGINE$NODE
export RUST_BACKTRACE=full

echo "[Node $NODE] Restarting node..."
cargo build
cargo run --bin $APP_BINARY -q -- start --home "$NODES_HOME/$NODE" --exec-engine "$EXEC_ENGINE" --config ".testnet/config/$NODE/config.toml" > "$NODES_HOME/$NODE/logs/node.log" 2>&1 &
echo $! > "$NODES_HOME/$NODE/node.pid"
echo "[Node $NODE] Node restarted with PID $(cat $NODES_HOME/$NODE/node.pid)"
echo "[Node $NODE] Logs are available at: $NODES_HOME/$NODE/logs/node.log"
