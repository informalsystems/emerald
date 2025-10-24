#!/bin/bash

# Script to compare state between full node and validator
# Usage: ./compare_nodes.sh [block_number] [fullnode_rpc] [validator_rpc]
#   If block_number is omitted or "auto", uses the minimum of both nodes' latest blocks

set -e

# Default values
FULLNODE_RPC=${2:-http://127.0.0.1:18545}
VALIDATOR_RPC=${3:-http://127.0.0.1:8545}
TEST_ADDRESS="0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"

# Get latest block numbers first
FULLNODE_LATEST=$(curl -s -X POST $FULLNODE_RPC -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' | jq -r '.result')

VALIDATOR_LATEST=$(curl -s -X POST $VALIDATOR_RPC -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' | jq -r '.result')

# Determine which block to check
if [ -z "$1" ] || [ "$1" == "auto" ]; then
    # Use the minimum of the two latest blocks
    if [ $((FULLNODE_LATEST)) -lt $((VALIDATOR_LATEST)) ]; then
        BLOCK_NUM=$((FULLNODE_LATEST))
    else
        BLOCK_NUM=$((VALIDATOR_LATEST))
    fi
    echo "Auto-detected common block: $BLOCK_NUM"
else
    BLOCK_NUM=$1
fi

# Convert block number to hex
BLOCK_HEX=$(printf "0x%x" $BLOCK_NUM)

echo "=========================================="
echo "Comparing nodes at block $BLOCK_NUM ($BLOCK_HEX)"
echo "Full node: $FULLNODE_RPC"
echo "Validator: $VALIDATOR_RPC"
echo "=========================================="
echo

# 1. Compare block hashes
echo "1. Block Hash Comparison:"
echo "-------------------------"
FULLNODE_HASH=$(curl -s -X POST $FULLNODE_RPC -H "Content-Type: application/json" \
  --data "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBlockByNumber\",\"params\":[\"$BLOCK_HEX\",false],\"id\":1}" | jq -r '.result.hash')

VALIDATOR_HASH=$(curl -s -X POST $VALIDATOR_RPC -H "Content-Type: application/json" \
  --data "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBlockByNumber\",\"params\":[\"$BLOCK_HEX\",false],\"id\":1}" | jq -r '.result.hash')

echo "Full node hash:  $FULLNODE_HASH"
echo "Validator hash:  $VALIDATOR_HASH"

if [ "$FULLNODE_HASH" == "$VALIDATOR_HASH" ]; then
    echo "✅ Block hashes MATCH"
else
    echo "❌ Block hashes DIFFER - nodes are on different forks!"
fi
echo

# 2. Compare latest block numbers
echo "2. Latest Block Numbers:"
echo "------------------------"
echo "Full node:  $((FULLNODE_LATEST)) (hex: $FULLNODE_LATEST)"
echo "Validator:  $((VALIDATOR_LATEST)) (hex: $VALIDATOR_LATEST)"

DIFF=$((VALIDATOR_LATEST - FULLNODE_LATEST))
echo "Difference: $DIFF blocks"
echo

# 3. Compare account nonces at specified block
echo "3. Account Nonce at Block $BLOCK_NUM:"
echo "--------------------------------------"
FULLNODE_NONCE=$(curl -s -X POST $FULLNODE_RPC -H "Content-Type: application/json" \
  --data "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getTransactionCount\",\"params\":[\"$TEST_ADDRESS\",\"$BLOCK_HEX\"],\"id\":1}" | jq -r '.result')

VALIDATOR_NONCE=$(curl -s -X POST $VALIDATOR_RPC -H "Content-Type: application/json" \
  --data "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getTransactionCount\",\"params\":[\"$TEST_ADDRESS\",\"$BLOCK_HEX\"],\"id\":1}" | jq -r '.result')

echo "Full node nonce:  $((FULLNODE_NONCE))"
echo "Validator nonce:  $((VALIDATOR_NONCE))"

if [ "$FULLNODE_NONCE" == "$VALIDATOR_NONCE" ]; then
    echo "✅ Nonces MATCH"
else
    echo "❌ Nonces DIFFER - state divergence detected!"
    NONCE_DIFF=$((VALIDATOR_NONCE - FULLNODE_NONCE))
    echo "   Difference: $NONCE_DIFF transactions"
fi
echo

# 4. Compare account nonces at latest block
echo "4. Account Nonce at Latest Block:"
echo "----------------------------------"
FULLNODE_NONCE_LATEST=$(curl -s -X POST $FULLNODE_RPC -H "Content-Type: application/json" \
  --data "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getTransactionCount\",\"params\":[\"$TEST_ADDRESS\",\"latest\"],\"id\":1}" | jq -r '.result')

VALIDATOR_NONCE_LATEST=$(curl -s -X POST $VALIDATOR_RPC -H "Content-Type: application/json" \
  --data "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getTransactionCount\",\"params\":[\"$TEST_ADDRESS\",\"latest\"],\"id\":1}" | jq -r '.result')

echo "Full node nonce:  $((FULLNODE_NONCE_LATEST))"
echo "Validator nonce:  $((VALIDATOR_NONCE_LATEST))"

if [ "$FULLNODE_NONCE_LATEST" == "$VALIDATOR_NONCE_LATEST" ]; then
    echo "✅ Latest nonces MATCH"
else
    echo "❌ Latest nonces DIFFER"
    NONCE_DIFF_LATEST=$((VALIDATOR_NONCE_LATEST - FULLNODE_NONCE_LATEST))
    echo "   Difference: $NONCE_DIFF_LATEST transactions"
fi
echo

# 5. Check sync status
echo "5. Sync Status:"
echo "---------------"
FULLNODE_SYNCING=$(curl -s -X POST $FULLNODE_RPC -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' | jq -r '.result')

VALIDATOR_SYNCING=$(curl -s -X POST $VALIDATOR_RPC -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' | jq -r '.result')

echo "Full node syncing:  $FULLNODE_SYNCING"
echo "Validator syncing:  $VALIDATOR_SYNCING"
echo

# 6. Check peer counts
echo "6. P2P Peer Counts:"
echo "-------------------"
FULLNODE_PEERS=$(curl -s -X POST $FULLNODE_RPC -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}' | jq -r '.result')

VALIDATOR_PEERS=$(curl -s -X POST $VALIDATOR_RPC -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}' | jq -r '.result')

echo "Full node peers:  $((FULLNODE_PEERS))"
echo "Validator peers:  $((VALIDATOR_PEERS))"
echo

# 7. Check txpool status
echo "7. Transaction Pool Status:"
echo "---------------------------"
FULLNODE_TXPOOL=$(curl -s -X POST $FULLNODE_RPC -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"txpool_status","params":[],"id":1}')

VALIDATOR_TXPOOL=$(curl -s -X POST $VALIDATOR_RPC -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"txpool_status","params":[],"id":1}')

FULLNODE_PENDING=$(echo $FULLNODE_TXPOOL | jq -r '.result.pending')
VALIDATOR_PENDING=$(echo $VALIDATOR_TXPOOL | jq -r '.result.pending')

echo "Full node pending:  $((FULLNODE_PENDING))"
echo "Validator pending:  $((VALIDATOR_PENDING))"
echo

# 8. Compare transaction count in the block
echo "8. Transaction Count in Block $BLOCK_NUM:"
echo "-----------------------------------------"
FULLNODE_TX_COUNT=$(curl -s -X POST $FULLNODE_RPC -H "Content-Type: application/json" \
  --data "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBlockTransactionCountByNumber\",\"params\":[\"$BLOCK_HEX\"],\"id\":1}" | jq -r '.result')

VALIDATOR_TX_COUNT=$(curl -s -X POST $VALIDATOR_RPC -H "Content-Type: application/json" \
  --data "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBlockTransactionCountByNumber\",\"params\":[\"$BLOCK_HEX\"],\"id\":1}" | jq -r '.result')

echo "Full node tx count:  $((FULLNODE_TX_COUNT))"
echo "Validator tx count:  $((VALIDATOR_TX_COUNT))"

if [ "$FULLNODE_TX_COUNT" == "$VALIDATOR_TX_COUNT" ]; then
    echo "✅ Transaction counts MATCH"
else
    echo "❌ Transaction counts DIFFER"
fi
echo

# Summary
echo "=========================================="
echo "SUMMARY"
echo "=========================================="

if [ "$FULLNODE_HASH" == "$VALIDATOR_HASH" ] && \
   [ "$FULLNODE_NONCE_LATEST" == "$VALIDATOR_NONCE_LATEST" ] && \
   [ "$FULLNODE_TX_COUNT" == "$VALIDATOR_TX_COUNT" ]; then
    echo "✅ Nodes appear to be in sync"
else
    echo "⚠️  Nodes have differences detected"
    echo ""
    echo "Possible issues:"
    if [ "$FULLNODE_HASH" != "$VALIDATOR_HASH" ]; then
        echo "  - Different forks at block $BLOCK_NUM"
    fi
    if [ "$FULLNODE_NONCE_LATEST" != "$VALIDATOR_NONCE_LATEST" ]; then
        echo "  - State divergence (nonce mismatch)"
    fi
    if [ $((FULLNODE_PENDING)) -gt 100 ]; then
        echo "  - Full node has stale transactions in mempool"
    fi
fi
echo
