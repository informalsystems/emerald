#!/usr/bin/env bash
# Run MBT tests for Emerald
# This script checks prerequisites and runs the MBT test suite

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR/../.."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== Emerald MBT Test Runner ===${NC}"
echo ""

# Check if reth is running
check_reth() {
    echo -e "${YELLOW}Checking if Reth is running...${NC}"

    if curl -s -X POST -H "Content-Type: application/json" \
        --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
        http://localhost:8545 > /dev/null 2>&1; then
        echo -e "${GREEN}✓ Reth is running${NC}"
        return 0
    else
        echo -e "${RED}✗ Reth is not running${NC}"
        return 1
    fi
}

# Function to wait for reth
wait_for_reth() {
    echo -e "${YELLOW}Waiting for Reth to start...${NC}"
    local max_attempts=30
    local attempt=0

    while [ $attempt -lt $max_attempts ]; do
        if check_reth 2>/dev/null; then
            return 0
        fi
        attempt=$((attempt + 1))
        echo -e "${YELLOW}  Attempt $attempt/$max_attempts...${NC}"
        sleep 2
    done

    echo -e "${RED}Failed to connect to Reth after $max_attempts attempts${NC}"
    return 1
}

# Check prerequisites
if ! check_reth; then
    echo ""
    echo -e "${YELLOW}Reth is not running. You need to start it first.${NC}"
    echo -e "${YELLOW}In a separate terminal, run:${NC}"
    echo -e "${BLUE}  cd $SCRIPT_DIR${NC}"
    echo -e "${BLUE}  ./start-reth.sh${NC}"
    echo ""

    read -p "$(echo -e ${YELLOW}Do you want to wait for Reth to start? [y/N]: ${NC})" -n 1 -r
    echo ""

    if [[ $REPLY =~ ^[Yy]$ ]]; then
        if ! wait_for_reth; then
            echo -e "${RED}Cannot proceed without Reth. Exiting.${NC}"
            exit 1
        fi
    else
        echo -e "${RED}Cannot run tests without Reth. Exiting.${NC}"
        exit 1
    fi
fi

echo ""
echo -e "${GREEN}=== Running MBT Tests ===${NC}"
echo ""

# Run the tests
cd "$SCRIPT_DIR"
cargo test --lib -- --nocapture "$@"

echo ""
echo -e "${GREEN}=== Tests Complete ===${NC}"
