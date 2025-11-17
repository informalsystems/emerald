#!/usr/bin/env bash
#
# validator-manager.sh - CLI for interacting with ValidatorManager contract
#
# Compatible with: Linux, macOS (bash 4+)
#
# Usage:
#   ./validator-manager.sh <command> [options]
#
# Commands:
#   register <validator_key> <power>    Register a new validator
#   unregister <validator_key>          Unregister an existing validator
#   update-power <validator_key> <power> Update validator's voting power
#   status                               Show current contract state
#   owner                                Show current contract owner
#   get <validator_key>                  Get specific validator info
#   is-validator <validator_key>         Check if key is a validator
#   transfer-ownership <new_owner>       Transfer contract ownership
#   renounce-ownership                   Renounce ownership (irreversible!)

set -euo pipefail

# Configuration
RPC_URL="${RPC_URL:-http://127.0.0.1:8645}"
VM_ADDRESS="${VM_ADDRESS:-0x0000000000000000000000000000000000002000}"
OWNER_KEY="${OWNER_KEY:-0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80}"
ABI_FILE="${ABI_FILE:-./solidity/out/ValidatorManager.sol/ValidatorManager.json}"

# Genesis validator keys (from assets/genesis.json)
VAL_KEY_0=0x681eaaa34e491e6c8335abc9ea92b024ef52eb91442ca3b84598c79a79f31b75
VAL_KEY_1=0x186d3eeda02ead5fbbad744eed158d958c4f7f48561a3e66cb1ee96855ad5c19
VAL_KEY_2=0x31ab5f0a05e248a493033047c8581f0079b4558f5e9c71af616ae76d80cbdb07

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper functions
info() {
    printf "${BLUE}[INFO]${NC} %s\n" "$*"
}

success() {
    printf "${GREEN}[OK]${NC} %s\n" "$*"
}

error() {
    printf "${RED}[ERROR]${NC} %s\n" "$*" >&2
}

warn() {
    printf "${YELLOW}[WARN]${NC} %s\n" "$*"
}

usage() {
    cat << EOF
ValidatorManager CLI

Usage: $0 <command> [options]

Commands:
  register <validator_key> <power>     Register a new validator
  unregister <validator_key>           Unregister an existing validator
  update-power <validator_key> <power> Update validator's voting power
  status                                Show current contract state
  owner                                 Show current contract owner
  get <validator_key>                   Get specific validator info
  is-validator <validator_key>          Check if key is a validator
  list-genesis-keys                     List the 3 genesis validator keys
  transfer-ownership <new_owner>        Transfer contract ownership
  renounce-ownership                    Renounce ownership (irreversible!)

Environment Variables:
  RPC_URL      RPC endpoint (default: http://127.0.0.1:8645)
  VM_ADDRESS   ValidatorManager contract address (default: 0x0000000000000000000000000000000000002000)
  OWNER_KEY    Owner private key (default: first Hardhat test account)
  ABI_FILE     Path to ValidatorManager ABI JSON (default: ./solidity/out/ValidatorManager.sol/ValidatorManager.json)

Examples:
  # Register a new validator with power 1000
  $0 register 0x1234...abcd 1000

  # Update validator power to 500
  $0 update-power 0x1234...abcd 500

  # Show current state
  $0 status

  # List genesis validator keys
  $0 list-genesis-keys

  # Check if a key is a validator
  $0 is-validator \$VAL_KEY_0

  # Unregister a validator
  $0 unregister 0x1234...abcd

EOF
    exit 1
}

# Command implementations
cmd_register() {
    local validator_key="$1"
    local power="$2"
    
    info "Registering validator..."
    info "  Key: $validator_key"
    info "  Power: $power"
    
    cast send \
        --rpc-url "$RPC_URL" \
        --private-key "$OWNER_KEY" \
        "$VM_ADDRESS" \
        "register(uint256,uint256)" \
        "$validator_key" \
        "$power"
    
    success "Validator registered successfully"
}

cmd_unregister() {
    local validator_key="$1"
    
    info "Unregistering validator..."
    info "  Key: $validator_key"
    
    cast send \
        --rpc-url "$RPC_URL" \
        --private-key "$OWNER_KEY" \
        "$VM_ADDRESS" \
        "unregister(uint256)" \
        "$validator_key"
    
    success "Validator unregistered successfully"
}

cmd_update_power() {
    local validator_key="$1"
    local new_power="$2"
    
    info "Updating validator power..."
    info "  Key: $validator_key"
    info "  New Power: $new_power"
    
    cast send \
        --rpc-url "$RPC_URL" \
        --private-key "$OWNER_KEY" \
        "$VM_ADDRESS" \
        "updatePower(uint256,uint256)" \
        "$validator_key" \
        "$new_power"
    
    success "Validator power updated successfully"
}

cmd_status() {
    info "ValidatorManager Status"
    echo ""
    
    # Get owner
    local owner
    owner=$(cast call --rpc-url "$RPC_URL" "$VM_ADDRESS" "owner()")
    # Extract last 40 hex chars (20 bytes) for address
    owner="0x${owner: -40}"
    owner=$(cast --to-checksum-address "$owner")
    echo "Owner:           $owner"
    
    # Get validator count
    local count
    count=$(cast call --rpc-url "$RPC_URL" "$VM_ADDRESS" "getValidatorCount()")
    count=$(cast --to-dec "$count")
    echo "Validator Count: $count"
    
    # Get total power
    local total_power
    total_power=$(cast call --rpc-url "$RPC_URL" "$VM_ADDRESS" "getTotalPower()")
    total_power=$(cast --to-dec "$total_power")
    echo "Total Power:     $total_power"
    
    # Get all validator keys and display each
    if [[ "$count" -gt 0 ]]; then
        echo ""
        info "Validators:"
        
        # Get validator keys array
        local keys_raw
        keys_raw=$(cast call --rpc-url "$RPC_URL" "$VM_ADDRESS" "getValidatorKeys()")
        keys_raw="${keys_raw#0x}"
        
        # Parse the dynamic array:
        # First 64 chars (32 bytes) = offset to data
        # Next 64 chars (32 bytes) = array length
        # Remaining = packed uint256 values (64 hex chars each)
        local array_count_hex="0x${keys_raw:64:64}"
        local array_count
        array_count=$(cast --to-dec "$array_count_hex")
        
        # Data starts at position 128
        local array_data="${keys_raw:128}"
        
        # Extract each 64-char (32-byte) key
        local i=0
        while [[ $i -lt $array_count ]]; do
            local start=$((i * 64))
            local hex_key="0x${array_data:start:64}"
            
            # Skip empty keys
            if [[ -z "${hex_key//0x/}" ]]; then
                ((i++))
                continue
            fi
            
            # Get the validator info for this key
            local validator_data power_hex power_dec
            if validator_data=$(cast call --rpc-url "$RPC_URL" "$VM_ADDRESS" "getValidator(uint256)" "$hex_key" 2>/dev/null); then
                validator_data="${validator_data#0x}"
                
                power_hex="0x${validator_data:64:64}"
                power_dec=$(cast --to-dec "$power_hex" 2>/dev/null || echo "0")
                
                echo "  [$((i + 1))] Key: $hex_key | Power: $power_dec"
            fi
            ((i++)) || true
        done
    else
        warn "No validators registered"
    fi
}

cmd_owner() {
    local owner
    owner=$(cast call --rpc-url "$RPC_URL" "$VM_ADDRESS" "owner()")
    # Extract last 40 hex chars (20 bytes) for address
    owner="0x${owner: -40}"
    owner=$(cast --to-checksum-address "$owner")
    echo "Current Owner: $owner"
}

cmd_get() {
    local validator_key="$1"
    
    info "Querying validator: $validator_key"
    
    local result
    if ! result=$(cast call --rpc-url "$RPC_URL" "$VM_ADDRESS" "getValidator(uint256)" "$validator_key" 2>&1); then
        error "Validator does not exist"
        exit 1
    fi
    
    echo ""
    # Cast returns: 0xKEY(64 hex chars)POWER(64 hex chars)
    # Remove 0x prefix first
    result="${result#0x}"
    
    # Extract key (first 64 chars) and power (next 64 chars)
    local hex_key="0x${result:0:64}"
    local hex_power="0x${result:64:64}"
    
    local power_dec
    power_dec=$(cast --to-dec "$hex_power" 2>/dev/null || echo "N/A")
    
    echo "Validator Key:   $hex_key"
    echo "Voting Power:    $power_dec"
}

cmd_is_validator() {
    local validator_key="$1"
    
    local result
    result=$(cast call --rpc-url "$RPC_URL" "$VM_ADDRESS" "isValidator(uint256)" "$validator_key")
    
    # Convert hex boolean to decimal (0 or 1)
    local is_validator
    is_validator=$(cast --to-dec "$result")
    
    if [[ "$is_validator" == "1" ]]; then
        success "$validator_key is a registered validator"
        return 0
    else
        warn "$validator_key is NOT a registered validator"
        return 1
    fi
}

cmd_list_genesis_keys() {
    info "Genesis Validator Keys (from assets/genesis.json):"
    echo ""
    echo "Validator 0: $VAL_KEY_0"
    echo "Validator 1: $VAL_KEY_1"
    echo "Validator 2: $VAL_KEY_2"
    echo ""
    info "Use these keys with other commands, e.g.:"
    echo "  $0 get $VAL_KEY_0"
    echo "  $0 update-power $VAL_KEY_1 200"
}

cmd_transfer_ownership() {
    local new_owner="$1"
    
    warn "Transferring ownership to: $new_owner"
    read -rp "Are you sure? (yes/no): " confirm
    
    if [[ "$confirm" != "yes" ]]; then
        error "Cancelled"
        exit 1
    fi
    
    cast send \
        --rpc-url "$RPC_URL" \
        --private-key "$OWNER_KEY" \
        "$VM_ADDRESS" \
        "transferOwnership(address)" \
        "$new_owner"
    
    success "Ownership transferred to $new_owner"
}

cmd_renounce_ownership() {
    error "WARNING: This will permanently lock all mutation functions!"
    error "No one will be able to register, unregister, or update validators."
    read -rp "Type 'RENOUNCE' to confirm: " confirm
    
    if [[ "$confirm" != "RENOUNCE" ]]; then
        error "Cancelled"
        exit 1
    fi
    
    cast send \
        --rpc-url "$RPC_URL" \
        --private-key "$OWNER_KEY" \
        "$VM_ADDRESS" \
        "renounceOwnership()"
    
    warn "Ownership renounced. Contract is now locked."
}

# Main command dispatcher
main() {
    if [[ $# -eq 0 ]]; then
        usage
    fi
    
    local command="$1"
    shift
    
    case "$command" in
        register)
            [[ $# -eq 2 ]] || { error "Usage: register <validator_key> <power>"; exit 1; }
            cmd_register "$1" "$2"
            ;;
        unregister)
            [[ $# -eq 1 ]] || { error "Usage: unregister <validator_key>"; exit 1; }
            cmd_unregister "$1"
            ;;
        update-power)
            [[ $# -eq 2 ]] || { error "Usage: update-power <validator_key> <power>"; exit 1; }
            cmd_update_power "$1" "$2"
            ;;
        status)
            cmd_status
            ;;
        owner)
            cmd_owner
            ;;
        get)
            [[ $# -eq 1 ]] || { error "Usage: get <validator_key>"; exit 1; }
            cmd_get "$1"
            ;;
        is-validator)
            [[ $# -eq 1 ]] || { error "Usage: is-validator <validator_key>"; exit 1; }
            cmd_is_validator "$1"
            ;;
        list-genesis-keys)
            cmd_list_genesis_keys
            ;;
        transfer-ownership)
            [[ $# -eq 1 ]] || { error "Usage: transfer-ownership <new_owner>"; exit 1; }
            cmd_transfer_ownership "$1"
            ;;
        renounce-ownership)
            cmd_renounce_ownership
            ;;
        help|--help|-h)
            usage
            ;;
        *)
            error "Unknown command: $command"
            echo ""
            usage
            ;;
    esac
}

main "$@"
