#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat <<EOF >&2
Usage: $0 [--nodes <number>] [--node-keys <key>]... --testnet-config-dir <path>

Required arguments:
    --testnet-config-dir   Directory where the testnet configuration should be written

Optional arguments:
    --nodes                Number of nodes to include in the generated config
    --node-keys            Private key for a node (can be specified multiple times)
                          If provided, the number of nodes is inferred from the number of keys
    --fee-recipient        Fee recipient address

Note: Either --nodes or --node-keys must be provided
EOF
    exit 2
}

nodes=""
testnet_config_dir=""
node_keys=()
fee_recipient="0x4242424242424242424242424242424242424242"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --nodes)
            [[ $# -ge 2 ]] || usage
            nodes="$2"
            shift 2
            ;;
        --nodes=*)
            nodes="${1#*=}"
            shift
            ;;
        --node-keys)
            [[ $# -ge 2 ]] || usage
            node_keys+=("$2")
            shift 2
            ;;
        --node-keys=*)
            node_keys+=("${1#*=}")
            shift
            ;;
        --testnet-config-dir)
            [[ $# -ge 2 ]] || usage
            testnet_config_dir="$2"
            shift 2
            ;;
        --testnet-config-dir=*)
            testnet_config_dir="${1#*=}"
            shift
            ;;
        --fee-recipient)
            [[ $# -ge 2 ]] || usage
            fee_recipient="$2"
            shift 2
            ;;
        --fee-recipient=*)
            fee_recipient="${1#*=}"
            shift
            ;;
        -h|--help)
            usage
            ;;
        --)
            shift
            break
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage
            ;;
    esac
done

# If node_keys are provided, infer the number of nodes
if [[ ${#node_keys[@]} -gt 0 ]]; then
    nodes="${#node_keys[@]}"
fi

if [[ -z "$nodes" || -z "$testnet_config_dir" ]]; then
    usage
fi

if ! [[ "$nodes" =~ ^[0-9]+$ ]] || (( nodes <= 0 )); then
    echo "--nodes must be a positive integer" >&2
    exit 2
fi

# Validate that if node_keys are provided, the count matches nodes
if [[ ${#node_keys[@]} -gt 0 && ${#node_keys[@]} -ne $nodes ]]; then
    echo "Number of node keys (${#node_keys[@]}) doesn't match number of nodes ($nodes)" >&2
    exit 2
fi

# Validate --fee-recipient format: must be 0x + 40 hex chars
if ! [[ "$fee_recipient" =~ ^0x[0-9a-fA-F]{40}$ ]]; then
    echo "Invalid --fee-recipient: must be 0x followed by 40 hex characters" >&2
    echo "Example: 0x4242424242424242424242424242424242424242" >&2
    exit 2
fi

TESTNET_DIR="$testnet_config_dir"

# Function to calculate engine port for a given node ID
get_engine_port() {
    local node_id=$1
    PORT=$((8645 + i * 100))
    if (( node_id == 0 )); then
        echo "8645"
    else
        echo "$PORT"
    fi
}

# Function to calculate auth port for a given node ID
get_auth_port() {
    local node_id=$1
    PORT=$((8551 + i * 1000))
    if (( node_id == 0 )); then
        echo "8551"
    else
        echo "$PORT"
    fi
}

mkdir -p "$TESTNET_DIR"
mkdir -p "$TESTNET_DIR/config"

cat > "$TESTNET_DIR/testnet_config.toml" <<EOF
nodes = $nodes
deterministic = true

EOF

# Add private_keys if provided
if [[ ${#node_keys[@]} -gt 0 ]]; then
    printf 'private_keys = [ ' >> "$TESTNET_DIR/testnet_config.toml"
    for ((i = 0; i < ${#node_keys[@]}; i++)); do
        printf '"%s"' "${node_keys[i]}" >> "$TESTNET_DIR/testnet_config.toml"
        if (( i < ${#node_keys[@]} - 1 )); then
            printf ', ' >> "$TESTNET_DIR/testnet_config.toml"
        else
            printf ' ]\n' >> "$TESTNET_DIR/testnet_config.toml"
        fi
    done
    printf '\n' >> "$TESTNET_DIR/testnet_config.toml"
fi

printf 'configuration_paths = [ ' >> "$TESTNET_DIR/testnet_config.toml"

for ((i = 0; i < nodes; i++)); do
    printf '"%s/config/%d/config.toml"' "$TESTNET_DIR" "$i" >> "$TESTNET_DIR/testnet_config.toml"
    if (( i < nodes - 1 )); then
        printf ', ' >> "$TESTNET_DIR/testnet_config.toml"
    else
        printf ' ]\n' >> "$TESTNET_DIR/testnet_config.toml"
    fi
done

printf 'monikers = [ ' >> "$TESTNET_DIR/testnet_config.toml"

for ((i = 0; i < nodes; i++)); do
    printf '"test-%d"' "$i" >> "$TESTNET_DIR/testnet_config.toml"
    if (( i < nodes - 1 )); then
        printf ', ' >> "$TESTNET_DIR/testnet_config.toml"
    else
        printf ' ]\n' >> "$TESTNET_DIR/testnet_config.toml"
    fi
done

PRUNING_NODES=() #list of nodes who we want pruned. Note that you need to set the correpsonding flags in compose.yaml

for ((i = 0; i < nodes; i++)); do
    mkdir -p "$TESTNET_DIR/config/$i"
    ENGINE_PORT=$(get_engine_port $i)
    AUTH_PORT=$(get_auth_port $i)
    cat > "$TESTNET_DIR/config/$i/config.toml" <<EOF
moniker = "test-$i"
execution_authrpc_address = "http://localhost:$ENGINE_PORT"
engine_authrpc_address = "http://localhost:$AUTH_PORT"
jwt_token_path = "./assets/jwtsecret"
sync_timeout_ms = 10000
sync_initial_delay_ms = 100
retry_config.initial_delay = "100ms"
retry_config.max_delay = "2s"
retry_config.max_elapsed_time = "20s"
EOF
 # Set max_retain_blocks for pruning nodes
      if [[ ${#PRUNING_NODES[@]} -gt 0 && " ${PRUNING_NODES[@]} " =~ " ${i} " ]]; then
          echo "max_retain_blocks = 10064" >> "$TESTNET_DIR/config/$i/config.toml"
          echo "el_node_type = \"custom\"" >> "$TESTNET_DIR/config/$i/config.toml"
      else
          echo "el_node_type = \"archive\"" >> "$TESTNET_DIR/config/$i/config.toml"
      fi
      if [[ -n "$fee_recipient" ]]; then
          echo "fee_recipient = \"$fee_recipient\"" >> "$TESTNET_DIR/config/$i/config.toml"
      fi
done
