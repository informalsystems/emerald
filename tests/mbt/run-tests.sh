#!/usr/bin/env bash
# Run MBT tests for Emerald
# This script checks prerequisites and runs the MBT test suite

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== Emerald MBT Test Runner ===${NC}"

# Check if reth is running
check_reth() {
    if curl -s -X POST -H "Content-Type: application/json" \
        --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
        http://localhost:8545 > /dev/null 2>&1; then
        return 0
    else
        return 1
    fi
}

# Function to wait for reth
wait_for_reth() {
    local max_attempts=30
    local attempt=0

    while [ $attempt -lt $max_attempts ]; do
        if check_reth 2>/dev/null; then
            return 0
        fi
        attempt=$((attempt + 1))
        echo -e "${YELLOW}Waiting for Reth to start ($attempt/$max_attempts)...${NC}"
        sleep 2
    done

    echo -e "${RED}Failed to connect to Reth after $max_attempts attempts${NC}"
    return 1
}

# Track whether we started reth
STARTED_RETH=false
RETH_PID=""

# Function to stop reth if we started it
cleanup_reth() {
    if [ "$STARTED_RETH" = true ] && [ -n "$RETH_PID" ]; then
        echo ""
        echo -e "${YELLOW}Shutting down Reth (PID: $RETH_PID)...${NC}"
        kill $RETH_PID 2>/dev/null || true
        # Wait a moment for graceful shutdown
        sleep 2
        # Force kill if still running
        kill -9 $RETH_PID 2>/dev/null || true
        echo -e "${GREEN}✓ Reth stopped${NC}"
    fi
}

# Set up trap to ensure cleanup on exit
trap cleanup_reth EXIT

# Check prerequisites
if ! check_reth; then
    echo -e "${YELLOW}Reth is not running. Starting it in the background...${NC}"

    # Start reth in the background, suppressing output
    "$SCRIPT_DIR/start-reth.sh" > /dev/null 2>&1 &
    RETH_PID=$!
    STARTED_RETH=true

    echo -e "${GREEN}✓ Reth started (PID: $RETH_PID)${NC}"
fi

# Wait for reth to be ready
if ! wait_for_reth; then
    echo -e "${RED}Reth failed to start properly. Exiting.${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Reth is running${NC}"

echo ""
echo -e "${GREEN}=== Running MBT Tests ===${NC}"
echo ""

# Run the tests
cd "$SCRIPT_DIR"
cargo test --lib -- --nocapture "$@"

echo -e "${GREEN}=== Tests Complete ===${NC}"
