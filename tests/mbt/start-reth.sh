#!/usr/bin/env bash
# Start custom-reth for MBT testing
# This script generates genesis files and starts a local instance of custom-reth

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR/../.."
RETH_DATA_DIR="$SCRIPT_DIR/.reth-data"
GENESIS_FILE="$PROJECT_ROOT/assets/genesis.json"
EMERALD_GENESIS_FILE="$PROJECT_ROOT/assets/emerald_genesis.json"
PUBKEYS_FILE="$PROJECT_ROOT/assets/validator_public_keys.txt"
JWT_SECRET="$PROJECT_ROOT/assets/jwtsecret"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Starting custom-reth for MBT testing...${NC}"

# Check if custom-reth is built (it can be in two locations)
RETH_BIN=""

# Check in custom-reth/target first (if built from custom-reth directory)
if [ -f "$PROJECT_ROOT/custom-reth/target/release/custom-reth" ]; then
    RETH_BIN="$PROJECT_ROOT/custom-reth/target/release/custom-reth"
    echo -e "${GREEN}Found release build in custom-reth/target${NC}"
elif [ -f "$PROJECT_ROOT/custom-reth/target/debug/custom-reth" ]; then
    RETH_BIN="$PROJECT_ROOT/custom-reth/target/debug/custom-reth"
    echo -e "${GREEN}Found debug build in custom-reth/target${NC}"
# Check in project root target (if built from workspace)
elif [ -f "$PROJECT_ROOT/target/release/custom-reth" ]; then
    RETH_BIN="$PROJECT_ROOT/target/release/custom-reth"
    echo -e "${GREEN}Found release build in project target${NC}"
elif [ -f "$PROJECT_ROOT/target/debug/custom-reth" ]; then
    RETH_BIN="$PROJECT_ROOT/target/debug/custom-reth"
    echo -e "${GREEN}Found debug build in project target${NC}"
else
    echo -e "${YELLOW}custom-reth not found, building now...${NC}"
    echo -e "${YELLOW}This will take 5-10 minutes on first build...${NC}"
    cd "$PROJECT_ROOT/custom-reth"
    cargo build --release --bin custom-reth
    RETH_BIN="$PROJECT_ROOT/custom-reth/target/release/custom-reth"

    if [ ! -f "$RETH_BIN" ]; then
        echo -e "${RED}Build failed! Binary not found at: $RETH_BIN${NC}"
        exit 1
    fi
    cd "$SCRIPT_DIR"
fi

echo -e "${GREEN}Using reth binary: $RETH_BIN${NC}"

# Create data directory if it doesn't exist
mkdir -p "$RETH_DATA_DIR"

# Check if JWT secret exists
if [ ! -f "$JWT_SECRET" ]; then
    echo -e "${RED}JWT secret not found at $JWT_SECRET${NC}"
    echo -e "${YELLOW}Creating JWT secret...${NC}"
    mkdir -p "$(dirname "$JWT_SECRET")"
    openssl rand -hex 32 > "$JWT_SECRET"
fi

echo -e "${GREEN}JWT secret: $JWT_SECRET${NC}"
echo -e "${GREEN}Data directory: $RETH_DATA_DIR${NC}"

# ============================================================
# GENESIS GENERATION
# ============================================================
echo -e "${YELLOW}Checking genesis files...${NC}"

if [ ! -f "$GENESIS_FILE" ]; then
    echo -e "${YELLOW}Genesis not found, generating...${NC}"

    # Clean data directory since we're generating a new genesis
    if [ -d "$RETH_DATA_DIR" ]; then
        echo -e "${YELLOW}Cleaning old Reth data directory...${NC}"
        rm -rf "$RETH_DATA_DIR"
        mkdir -p "$RETH_DATA_DIR"
    fi

    # Build emerald-utils if not already built
    EMERALD_UTILS_BIN=""
    if [ -f "$PROJECT_ROOT/target/release/emerald-utils" ]; then
        EMERALD_UTILS_BIN="$PROJECT_ROOT/target/release/emerald-utils"
    elif [ -f "$PROJECT_ROOT/target/debug/emerald-utils" ]; then
        EMERALD_UTILS_BIN="$PROJECT_ROOT/target/debug/emerald-utils"
    else
        echo -e "${YELLOW}emerald-utils not found, building...${NC}"
        cd "$PROJECT_ROOT"
        cargo build --bin emerald-utils
        EMERALD_UTILS_BIN="$PROJECT_ROOT/target/debug/emerald-utils"
        cd "$SCRIPT_DIR"
    fi

    echo -e "${GREEN}Using emerald-utils: $EMERALD_UTILS_BIN${NC}"

    # Generate the test validator public keys using the same deterministic seeds as the driver
    # The driver uses StdRng::seed_from_u64(0, 1, 2) to generate 3 validators
    echo -e "${YELLOW}Generating deterministic validator public keys...${NC}"

    cd "$SCRIPT_DIR"
    cargo run --bin generate-validator-keys > "$PUBKEYS_FILE" 2>/dev/null || {
        echo -e "${RED}Failed to generate validator keys${NC}"
        echo -e "${YELLOW}Building MBT package first...${NC}"
        cargo build --bin generate-validator-keys
        cargo run --bin generate-validator-keys > "$PUBKEYS_FILE"
    }

    # Generate genesis files (using emerald-utils defaults for output paths)
    echo -e "${YELLOW}Calling emerald-utils genesis...${NC}"
    cd "$PROJECT_ROOT"
    "$EMERALD_UTILS_BIN" genesis \
        --public-keys-file "$PUBKEYS_FILE" \
        --poa-owner-address "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"

    echo -e "${GREEN}✓ Genesis files generated at:${NC}"
    echo -e "${GREEN}  - $GENESIS_FILE${NC}"
    echo -e "${GREEN}  - $EMERALD_GENESIS_FILE${NC}"
    cd "$SCRIPT_DIR"
else
    echo -e "${GREEN}✓ Using existing genesis at $GENESIS_FILE${NC}"
    echo -e "${YELLOW}  To regenerate: rm -f $GENESIS_FILE $EMERALD_GENESIS_FILE $PUBKEYS_FILE && rm -rf $RETH_DATA_DIR${NC}"
fi

# ============================================================
# START RETH
# ============================================================
echo -e "${GREEN}Starting reth with custom genesis...${NC}"

# Start reth with custom genesis
exec "$RETH_BIN" node \
    --chain="$GENESIS_FILE" \
    --dev \
    --dev.block-time=1s \
    --http \
    --http.addr=127.0.0.1 \
    --http.port=8545 \
    --http.api=eth,net,web3,debug,txpool,trace \
    --http.corsdomain="*" \
    --authrpc.addr=127.0.0.1 \
    --authrpc.port=8551 \
    --authrpc.jwtsecret="$JWT_SECRET" \
    --datadir="$RETH_DATA_DIR" \
    --log.stdout.filter=debug
