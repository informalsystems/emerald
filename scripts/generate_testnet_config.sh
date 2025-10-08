#!/usr/bin/env bash

set -e

if [ "$#" -lt 1 ]; then
  echo "Usage: $0 <number of nodes>" >&2
  exit 2
fi

nodes=$1

TESTNET_DIR="$HOME/.testnet"

ENGINE_PORT_0=8545
AUTH_PORT_0=8551

ENGINE_PORT_1=18545
AUTH_PORT_1=18551

ENGINE_PORT_2=28545
AUTH_PORT_2=28551

mkdir -p $TESTNET_DIR
mkdir -p $TESTNET_DIR/config

cat > $TESTNET_DIR/testnet_config.toml <<EOF
nodes = $nodes
deterministic = true

EOF

echo -n "configuration_paths = [ " >> $TESTNET_DIR/testnet_config.toml

for ((i = 0; i < nodes; i++)); do
    echo -n "\"$TESTNET_DIR/config/$i/config.toml\" " >> $TESTNET_DIR/testnet_config.toml
if [ $i -lt $((nodes - 1)) ]; then
    echo -n ","  >> $TESTNET_DIR/testnet_config.toml
else
    echo -n "]" >> $TESTNET_DIR/testnet_config.toml
fi
done

mkdir -p $TESTNET_DIR/config/0
cat > $TESTNET_DIR/config/0/config.toml <<EOF
moniker = "test-0"
execution_authrpc_address = "http://localhost:$ENGINE_PORT_0"
engine_authrpc_address = "http://localhost:$AUTH_PORT_0"
jwt_token_path = "./assets/jwtsecret"
EOF

mkdir -p $TESTNET_DIR/config/1
cat > $TESTNET_DIR/config/1/config.toml <<EOF
moniker = "test-1"
execution_authrpc_address = "http://localhost:$ENGINE_PORT_1"
engine_authrpc_address = "http://localhost:$AUTH_PORT_1"
jwt_token_path = "./assets/jwtsecret"
EOF

mkdir -p $TESTNET_DIR/config/2
cat > $TESTNET_DIR/config/2/config.toml <<EOF
moniker = "test-2"
execution_authrpc_address = "http://localhost:$ENGINE_PORT_2"
engine_authrpc_address = "http://localhost:$AUTH_PORT_2"
jwt_token_path = "./assets/jwtsecret"
EOF
