#!/bin/bash

# Script to check for nonce gaps in txpool
# Usage: ./check_nonce_gaps.sh [rpc_url] [address]

set -e

RPC_URL=${1:-http://127.0.0.1:8545}
ADDRESS=${2:-0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266}

echo "=========================================="
echo "Nonce Gap Analysis"
echo "=========================================="
echo "RPC: $RPC_URL"
echo "Address: $ADDRESS"
echo ""

# Get current nonce
CURRENT_NONCE=$(curl -s -X POST $RPC_URL -H "Content-Type: application/json" \
  --data "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getTransactionCount\",\"params\":[\"$ADDRESS\",\"latest\"],\"id\":1}" | \
  jq -r '.result' | xargs printf "%d\n")

echo "Current nonce: $CURRENT_NONCE"
echo ""

# Get pending nonces
echo "=== Pending Transactions ==="
PENDING_NONCES=$(curl -s -X POST $RPC_URL -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"txpool_content","params":[],"id":1}' | \
  jq -r ".result.pending[\"${ADDRESS,,}\"] | keys | map(tonumber) | sort | .[]" 2>/dev/null || echo "")

if [ -z "$PENDING_NONCES" ]; then
    echo "No pending transactions"
else
    echo "$PENDING_NONCES" | head -20
    PENDING_COUNT=$(echo "$PENDING_NONCES" | wc -l)
    echo "... ($PENDING_COUNT total)"
fi
echo ""

# Get queued nonces
echo "=== Queued Transactions ==="
QUEUED_NONCES=$(curl -s -X POST $RPC_URL -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"txpool_content","params":[],"id":1}' | \
  jq -r ".result.queued[\"${ADDRESS,,}\"] | keys | map(tonumber) | sort | .[]" 2>/dev/null || echo "")

if [ -z "$QUEUED_NONCES" ]; then
    echo "No queued transactions"
else
    echo "$QUEUED_NONCES"
    QUEUED_COUNT=$(echo "$QUEUED_NONCES" | wc -l)
    echo "($QUEUED_COUNT total)"
fi
echo ""

# Analyze gaps
echo "=== Gap Analysis ==="

if [ -n "$PENDING_NONCES" ]; then
    FIRST_PENDING=$(echo "$PENDING_NONCES" | head -1)
    LAST_PENDING=$(echo "$PENDING_NONCES" | tail -1)

    if [ $FIRST_PENDING -ne $CURRENT_NONCE ]; then
        echo "⚠️  GAP DETECTED: Pending starts at $FIRST_PENDING but current nonce is $CURRENT_NONCE"
        echo "   Missing nonces: $CURRENT_NONCE to $(($FIRST_PENDING - 1))"
    else
        echo "✅ No gap at start of pending (starts at current nonce)"
    fi

    # Check for gaps within pending
    EXPECTED=$CURRENT_NONCE
    for NONCE in $PENDING_NONCES; do
        if [ $NONCE -ne $EXPECTED ]; then
            echo "⚠️  GAP in pending: Expected $EXPECTED, found $NONCE"
            echo "   Missing nonces: $EXPECTED to $(($NONCE - 1))"
        fi
        EXPECTED=$(($NONCE + 1))
    done
fi

if [ -n "$QUEUED_NONCES" ]; then
    FIRST_QUEUED=$(echo "$QUEUED_NONCES" | head -1)
    LAST_QUEUED=$(echo "$QUEUED_NONCES" | tail -1)

    if [ -n "$PENDING_NONCES" ]; then
        EXPECTED_AFTER_PENDING=$(($LAST_PENDING + 1))
        if [ $FIRST_QUEUED -ne $EXPECTED_AFTER_PENDING ]; then
            echo "⚠️  GAP between pending and queued"
            echo "   Last pending: $LAST_PENDING"
            echo "   First queued: $FIRST_QUEUED"
            echo "   Missing nonces: $EXPECTED_AFTER_PENDING to $(($FIRST_QUEUED - 1))"
        fi
    else
        if [ $FIRST_QUEUED -ne $CURRENT_NONCE ]; then
            echo "⚠️  GAP DETECTED: Queued starts at $FIRST_QUEUED but current nonce is $CURRENT_NONCE"
            echo "   Missing nonces: $CURRENT_NONCE to $(($FIRST_QUEUED - 1))"
        fi
    fi
fi

echo ""
echo "=========================================="
echo "Summary"
echo "=========================================="
echo "Current nonce: $CURRENT_NONCE"
[ -n "$PENDING_NONCES" ] && echo "Pending range: $FIRST_PENDING-$LAST_PENDING ($PENDING_COUNT txs)"
[ -n "$QUEUED_NONCES" ] && echo "Queued range: $FIRST_QUEUED-$LAST_QUEUED ($QUEUED_COUNT txs)"
