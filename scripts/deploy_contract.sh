#!/usr/bin/env bash
set -euo pipefail
set -x

# --- Configuration ---
RPC_URL="http://localhost:8545"
SCRIPT_PATH="solidity/script/Counter.s.sol:CounterScript"
# ----------------------

MNEMONIC="${MNEMONIC:-test test test test test test test test test test test junk}"
PRIVATE_KEY="$(cast wallet derive-private-key --mnemonic "$MNEMONIC")"
ADDR="$(cast wallet address --private-key "$PRIVATE_KEY")"
cast balance "$ADDR" --rpc-url "$RPC_URL"
echo "Using deployer address: $ADDR"

sleep 2

# Deploy contract to an already running local node (e.g., another anvil)
echo "==> Deploying Counter using forge..."
forge script "$SCRIPT_PATH" --rpc-url "$RPC_URL" --broadcast --private-key "$PRIVATE_KEY" -vvvv --json > deploy_output.json
echo "Done"

# Extract the contract address
CONTRACT_ADDR="$(
  jq -r '
    # emit any string field that looks like an address from either naming style
    ..
    | .contract_address? // .contractAddress? // empty
    | select(type=="string" and test("^0x[0-9a-fA-F]{40}$"))
  ' deploy_output.json | head -n1
)"

if [[ -z "$CONTRACT_ADDR" ]]; then
  echo "❌ Could not find contract address in deploy_output.json"
  exit 1
fi

echo "✅ Deployed Counter at: $CONTRACT_ADDR"

cast call "$CONTRACT_ADDR" "number()" --rpc-url "$RPC_URL"
cast send "$CONTRACT_ADDR" "increment()" --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY"
cast call "$CONTRACT_ADDR" "number()" --rpc-url "$RPC_URL"
