#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat <<EOF >&2
Usage: $0 --nodes <number> --testnet-config-dir <path>

Required arguments:
    --nodes                Number of nodes to include in the generated config
    --testnet-config-dir   Directory where the testnet configuration should be written
EOF
    exit 2
}

nodes=""
testnet_config_dir=""

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
        --testnet-config-dir)
            [[ $# -ge 2 ]] || usage
            testnet_config_dir="$2"
            shift 2
            ;;
        --testnet-config-dir=*)
            testnet_config_dir="${1#*=}"
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

if [[ -z "$nodes" || -z "$testnet_config_dir" ]]; then
    usage
fi

if ! [[ "$nodes" =~ ^[0-9]+$ ]] || (( nodes <= 0 )); then
    echo "--nodes must be a positive integer" >&2
    exit 2
fi

TESTNET_DIR="$testnet_config_dir"

readonly ENGINE_PORTS=(8545 8546 8547 8548)
readonly AUTH_PORTS=(8551 8552 8553 8554)

if (( nodes > ${#ENGINE_PORTS[@]} )); then
    echo "This script currently supports up to ${#ENGINE_PORTS[@]} nodes" >&2
    exit 2
fi

mkdir -p "$TESTNET_DIR"
mkdir -p "$TESTNET_DIR/config"

cat > "$TESTNET_DIR/testnet_config.toml" <<EOF
nodes = $nodes
deterministic = true

EOF

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

for ((i = 0; i < nodes; i++)); do
    mkdir -p "$TESTNET_DIR/config/$i"
    cat > "$TESTNET_DIR/config/$i/config.toml" <<EOF
moniker = "test-$i"
execution_authrpc_address = "http://localhost:${ENGINE_PORTS[i]}"
engine_authrpc_address = "http://localhost:${AUTH_PORTS[i]}"
jwt_token_path = "/home/emerald/jwt"
sync_timeout_ms = 1000000
sync_initial_delay_ms = 100
el_node_type = "archive"
EOF
done
