#!/bin/bash

EXEC_ENGINE="reth"
COMPOSE_FILE="compose.yaml"

# Check if node ID argument is provided
if [ -z "$1" ]; then
    echo "Usage: $0 <node_id>"
    exit 1
fi

# Check if node ID argument is provided
if [ "$2" ]; then
EXEC_ENGINE=$2
COMPOSE_FILE="compose_$EXEC_ENGINE.yaml"
fi

echo "execution engine set to $EXEC_ENGINE, using compose file $COMPOSE_FILE" 

NODE_ID=$1
PID_FILE="nodes/$NODE_ID/node.pid"

echo "Stopping $EXEC_ENGINE node"


docker compose -f $COMPOSE_FILE stop $EXEC_ENGINE$NODE_ID

# Check if PID file exists
if [ ! -f "$PID_FILE" ]; then
    echo "Error: PID file not found at $PID_FILE"
    exit 1
fi

# Read PID from file
PID=$(cat "$PID_FILE")

if [ -z "$PID" ]; then
    echo "Error: PID file is empty"
    exit 1
fi

echo "Found PID $PID for node $NODE_ID"

# Check if process is running
if ! ps -p "$PID" > /dev/null 2>&1; then
    echo "Warning: Process $PID is not running"
    exit
fi

# Kill the process
echo "Killing process $PID..."
kill -9 "$PID"

if [ $? -eq 0 ]; then
    echo "Successfully killed process $PID"
else
    echo "Error: Failed to kill process $PID"
    exit 1
fi
