# Spammer Template System Documentation

## Overview

The spammer template system is a YAML-based transaction configuration framework that enables automated, high-throughput transaction spamming for Ethereum-based protocols. It supports round-robin execution of predefined transaction patterns to simulate realistic protocol usage and stress testing.

## What Are Spammer Templates?

Spammer templates are YAML configuration files that define sequences of Ethereum transactions. The spammer tool:
1. Loads these templates at runtime
2. Cycles through them in round-robin fashion
3. Signs and sends transactions at a configurable rate (transactions per second)
4. Tracks and reports statistics (successes, failures, gas usage)

**Key Use Cases:**
- DEX order book stress testing
- Protocol interaction simulation
- Load testing blockchain infrastructure
- Gas usage analysis
- Mempool behavior analysis

---

## Template Structure

### Basic YAML Format

```yaml
transactions:
  - type: eip1559
    to: "0x..."
    value: "0.001"
    gas_limit: 100000
    max_fee_per_gas: "5"
    max_priority_fee_per_gas: "2"
    input: "0x..."
```

### Supported Transaction Types

#### 1. EIP-1559 Transactions
Standard Ethereum transactions with EIP-1559 fee market.

**Required Fields:**
- `type`: Must be `eip1559`
- `to`: Recipient address (contract or EOA) in hex format
- `value`: ETH amount to send as string (e.g., "0.001" = 0.001 ETH)
- `gas_limit`: Maximum gas units (integer)
- `max_fee_per_gas`: Maximum fee per gas in gwei (string, e.g., "5")
- `max_priority_fee_per_gas`: Priority fee (tip) in gwei (string, e.g., "2")
- `input`: Hex-encoded calldata (can be empty "" for simple transfers)

**Example - Simple ETH Transfer:**
```yaml
- type: eip1559
  to: "0x0000000000000000000000000000000000000005"
  value: "0.001"
  gas_limit: 21000
  max_fee_per_gas: "2"
  max_priority_fee_per_gas: "1"
  input: ""
```

**Example - Contract Interaction:**
```yaml
- type: eip1559
  to: "0xD8E47743AC7C5eaE9A8e0C570b91A28FdB3f8F71"
  value: "0.0"
  gas_limit: 500000
  max_fee_per_gas: "5"
  max_priority_fee_per_gas: "2"
  input: "0x1cfe874000000000000000000000000000000000000000000000000000000000000005dc"
```

#### 2. EIP-4844 Transactions (Blob Transactions)
For protocols utilizing blob data storage.

**Required Fields:**
- `type`: Must be `eip4844`
- `to`: Recipient address
- `value`: ETH amount
- `gas_limit`: Maximum gas
- `max_fee_per_gas`: Maximum fee per gas in gwei
- `max_priority_fee_per_gas`: Priority fee in gwei
- `max_fee_per_blob_gas`: Maximum fee per blob gas in gwei

**Note:** Blob transactions automatically include random blob versioned hashes. Input data defaults to empty.

---

## Field Details

### Address Format
- **Format:** Hex string with "0x" prefix
- **Length:** 40 hex characters (20 bytes)
- **Example:** `"0xD8E47743AC7C5eaE9A8e0C570b91A28FdB3f8F71"`

### Value (ETH Amount)
- **Format:** String representing ETH amount
- **Unit:** ETH (not Wei)
- **Examples:**
  - `"0.001"` = 0.001 ETH = 1,000,000,000,000,000 Wei
  - `"1"` = 1 ETH
  - `"0.0"` = No ETH transfer

### Gas Fields
- **gas_limit:** Integer, maximum gas units for transaction execution
- **max_fee_per_gas:** String in gwei (e.g., `"5"` = 5 gwei = 5,000,000,000 Wei)
- **max_priority_fee_per_gas:** String in gwei, miner tip

**Gas Limit Guidelines:**
- Simple transfer: 21,000
- ERC-20 transfer: ~65,000
- Complex contract calls: 100,000 - 500,000
- Very complex operations: 500,000+

### Input Data (Calldata)
- **Format:** Hex string with "0x" prefix
- **Empty:** Use `""` for simple transfers
- **Structure:** `0x` + function selector (4 bytes) + encoded parameters

**How to Generate Input Data:**

1. **Get Function Selector:**
   - First 4 bytes of keccak256 hash of function signature
   - Example: `keccak256("initPVnode(uint32)")` = `0x1cfe8740...`

2. **Encode Parameters:**
   - ABI-encode parameters according to function signature
   - Each parameter is 32 bytes (64 hex chars)

3. **Example Breakdown:**
   ```
   Function: initPVnode(uint32 price)
   Selector: 0x1cfe8740
   Parameter: 1500 (uint32)
   Encoded: 00000000000000000000000000000000000000000000000000000000000005dc

   Full input: 0x1cfe874000000000000000000000000000000000000000000000000000000000000005dc
   ```

**Using `cast` to Generate Calldata:**
```bash
# For a function with parameters
cast calldata "newSellOrder(uint32,uint256,uint256)" 1500 50000000 0

# For a function without parameters
cast calldata "initPVnode(uint32)" 1500
```

---

## How Round-Robin Execution Works

The spammer uses a `RoundRobinSelector` that cycles through templates sequentially:

```
Template 1 → Template 2 → Template 3 → Template 1 → Template 2 → ...
```

**Example Execution:**
```yaml
transactions:
  - type: eip1559  # Transaction 1: Approval
    to: "0xTokenAddress"
    input: "0x095ea7b3..."  # approve()

  - type: eip1559  # Transaction 2: Sell Order
    to: "0xDEXAddress"
    input: "0x4cc4b233..."  # newSellOrder()

  - type: eip1559  # Transaction 3: Buy Order
    to: "0xDEXAddress"
    input: "0xd1a6e82c..."  # newBuyOrder()
```

If you run with `--rate 10 --num-txs 30`:
- Sends 10 transactions per second
- Total: 30 transactions
- Pattern: T1, T2, T3, T1, T2, T3, ... (10 complete cycles)

**Nonce Management:**
- Automatically increments nonce for each transaction
- Monitors pending nonce from node
- Re-syncs if nonce mismatch detected
- Ensures sequential execution

---

## Real-World Examples

### Example 1: Exchange DEX Testing
**File:** `utils/examples/exchange_transactions.yaml`

**Protocol:** Custom order book DEX
**Contract Functions:**
- `initPVnode(uint32)` - Initialize price level
- `newSellOrder(uint32,uint256,uint256)` - Place sell order
- `newBuyOrder(uint32,uint256,uint256)` - Place buy order

**Transaction Flow:**
```yaml
transactions:
  # 1. Initialize price level (runs once, then reverts)
  - type: eip1559
    to: "0xD8E47743AC7C5eaE9A8e0C570b91A28FdB3f8F71"
    input: "0x1cfe874000000000000000000000000000000000000000000000000000000000000005dc"
    # Decoded: initPVnode(1500)

  # 2. Place sell order: 50 tokens @ price 1500
  - type: eip1559
    to: "0xD8E47743AC7C5eaE9A8e0C570b91A28FdB3f8F71"
    input: "0x4cc4b233...0002faf080...00000000"
    # Decoded: newSellOrder(1500, 50000000, 0)

  # 3. Place buy order: 25 tokens @ price 1500 (matches sell)
  - type: eip1559
    to: "0xD8E47743AC7C5eaE9A8e0C570b91A28FdB3f8F71"
    input: "0xd1a6e82c...0017d7840...00000000"
    # Decoded: newBuyOrder(1500, 25000000, 0)
```

**Behavior:**
- Continuously places orders that match and execute
- Simulates active trading
- Tests order matching engine under load

### Example 2: Rubicon DEX Testing
**File:** `utils/examples/rubicon_dex_transactions.yaml`

**Protocol:** RubiconMarket (order book DEX)
**Function:** `offer(uint256,address,uint256,address,uint256,bool)`

**Transaction Flow:**
```yaml
transactions:
  # 1. Approve WETH spending
  - type: eip1559
    to: "0x5FbDB2315678afecb367f032d93F642f64180aa3"  # WETH9
    input: "0x095ea7b3...ffffffff"  # approve(spender, max)

  # 2. Approve USDC spending
  - type: eip1559
    to: "0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512"  # USDC
    input: "0x095ea7b3...ffffffff"

  # 3. Deposit ETH to get WETH
  - type: eip1559
    to: "0x5FbDB2315678afecb367f032d93F642f64180aa3"
    value: "0.001"
    input: "0xd0e30db0"  # deposit()

  # 4-13. Multiple SELL offers at varying prices
  - type: eip1559
    to: "0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0"
    input: "0xe1a6f014..."  # offer() - 0.01 WETH for 30 USDC

  # 14. Mint USDC for buying
  - type: eip1559
    to: "0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512"
    input: "0x6d1b229d"  # adminMint()

  # 15-22. Multiple BUY offers at varying prices (match sells)
  - type: eip1559
    to: "0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0"
    input: "0xe1a6f014..."  # offer() - 30 USDC for 0.01 WETH
```

**Key Features:**
- Includes approvals and setup transactions
- Creates realistic order book depth with multiple price levels
- Alternates between selling and buying to simulate trading

### Example 3: Simple Transfers
**File:** `utils/examples/simple_transactions.yaml`

**Purpose:** Basic testing without protocol-specific logic

```yaml
transactions:
  # Simple ETH transfer
  - type: eip1559
    to: "0x0000000000000000000000000000000000000005"
    value: "0.001"
    gas_limit: 21000
    max_fee_per_gas: "2"
    max_priority_fee_per_gas: "1"
    input: ""
```

---

## Running the Spammer

### Command Structure
```bash
cargo run --bin malachitebft-eth-utils -- spam \
  --dex \
  --template <path-to-yaml> \
  --rate <tx-per-second> \
  --num-txs <total-txs> \
  --time <seconds> \
  --rpc-url <url> \
  --signer-index <index>
```

### Parameters

- `--dex`: Enable DEX mode (loads template)
- `--template <path>`: Path to YAML template file
  - Default: `utils/examples/exchange_transactions.yaml`
- `--rate <n>`: Transactions per second (default: 1000)
- `--num-txs <n>`: Total number of transactions (0 = unlimited)
- `--time <n>`: Maximum runtime in seconds (0 = unlimited)
- `--rpc-url <url>`: Ethereum RPC endpoint (default: 127.0.0.1:8545)
- `--signer-index <n>`: Index of signer from predefined signers (default: 0)
- `--blobs`: Send EIP-4844 blob transactions (without template mode)

### Example Commands

**DEX Spamming (default template):**
```bash
cargo run --bin malachitebft-eth-utils -- spam \
  --dex \
  --rate 10 \
  --num-txs 30 \
  --rpc-url 127.0.0.1:8545 \
  --signer-index 0
```

**Custom Template:**
```bash
cargo run --bin malachitebft-eth-utils -- spam \
  --dex \
  --template utils/examples/rubicon_dex_transactions.yaml \
  --rate 5 \
  --num-txs 100 \
  --rpc-url 127.0.0.1:8545
```

**Simple Contract Spamming (without template):**
```bash
cargo run --bin malachitebft-eth-utils -- spam-contract \
  --contract 0x5FbDB2315678afecb367f032d93F642f64180aa3 \
  --function "increment()" \
  --rate 100 \
  --num-txs 1000 \
  --rpc-url 127.0.0.1:8545
```

### Output and Logging

**Console Output:**
```
Logging to: ./utils/logs/dex_spammer_20250110_143052.log
```

**Log Format:**
```
[0] elapsed 1.234s: Sent 10 txs (5240 bytes)
[0] elapsed 2.456s: Sent 20 txs (10480 bytes); 2 failed with {"insufficient funds": 2}
```

**Statistics Tracked:**
- Transactions sent (success count)
- Total bytes sent
- Failed transactions by error type
- Elapsed time
- Txpool status (pending/queued)

---

## Prerequisites for Running Templates

### 1. Token Approvals
**Required for:** DEX interactions, ERC-20 token operations

**Why:** Contracts need permission to spend your tokens

**Example:**
```bash
# Approve DEX to spend Token A
cast send <TOKEN_ADDRESS> \
  "approve(address,uint256)" <DEX_ADDRESS> \
  $(cast max-uint) \
  --private-key $WALLET_PRIVATE_KEY \
  --rpc-url http://127.0.0.1:8545
```

**Template Inclusion:**
Some templates include approval transactions at the start (e.g., Rubicon example).

### 2. Token Balances
**Required for:** Trading, selling, liquidity provision

**Ensure signer has:**
- Native token (ETH) for gas
- Protocol-specific tokens for operations
- Sufficient balance for all planned transactions

**Check Balance:**
```bash
cast balance <ADDRESS> --rpc-url http://127.0.0.1:8545
cast call <TOKEN_ADDRESS> "balanceOf(address)" <ADDRESS> --rpc-url http://127.0.0.1:8545
```

### 3. Protocol State
**Required for:** Operations depending on existing state

**Examples:**
- Price indices must exist before placing orders
- Liquidity pools must be initialized
- Contracts must be deployed

**Verification:**
```bash
# Check if price level exists in DEX
cast call <EXCHANGE_ADDRESS> "getIndexOfPrice(uint32)" 1500 --rpc-url http://127.0.0.1:8545
```

### 4. Network Configuration
- RPC endpoint must be accessible
- Chain ID matches signer configuration (default: 1)
- Sufficient network throughput for high tx rates

---

## Creating Templates for New Protocols

### Information Checklist

When creating a spammer template for a new protocol, gather:

#### 1. Contract Information
- [ ] Contract address(es)
- [ ] ABI or function signatures
- [ ] Function selectors (first 4 bytes of function signature hash)

#### 2. Token Information (if applicable)
- [ ] Token addresses (e.g., base token, quote token)
- [ ] Token decimals
- [ ] Approval requirements

#### 3. Function Parameters
For each function to call:
- [ ] Function signature (e.g., `"swap(uint256,uint256)"`)
- [ ] Parameter types and valid ranges
- [ ] Parameter encoding (how to convert values to calldata)

#### 4. Protocol Logic
- [ ] Initialization requirements (setup transactions)
- [ ] State dependencies (what must exist before certain operations)
- [ ] Transaction ordering constraints
- [ ] Expected success/failure patterns

#### 5. Gas Requirements
- [ ] Typical gas usage per function
- [ ] Current network gas prices
- [ ] Gas limit safety margins

#### 6. Testing Parameters
- [ ] Realistic transaction amounts
- [ ] Appropriate transaction rates
- [ ] Expected behavior under load

### Step-by-Step Template Creation

#### Step 1: Analyze Protocol Functions

**Example: Uniswap V2 Swap**

Function signature:
```solidity
function swapExactTokensForTokens(
    uint amountIn,
    uint amountOutMin,
    address[] calldata path,
    address to,
    uint deadline
) external returns (uint[] memory amounts);
```

#### Step 2: Encode Function Calls

**Using `cast`:**
```bash
# Get function selector
cast sig "swapExactTokensForTokens(uint256,uint256,address[],address,uint256)"
# Output: 0x38ed1739

# Generate full calldata
cast calldata "swapExactTokensForTokens(uint256,uint256,address[],address,uint256)" \
  1000000 \
  950000 \
  "[0xToken0,0xToken1]" \
  0xRecipient \
  1735660800
```

**Manual Encoding:**
```
Selector: 0x38ed1739 (4 bytes)
Param 1: amountIn = 1000000 (uint256, 32 bytes)
         0000000000000000000000000000000000000000000000000000000000f42400
Param 2: amountOutMin = 950000 (uint256, 32 bytes)
         00000000000000000000000000000000000000000000000000000000000e7be0
Param 3: path (dynamic array, requires offset + length + data)
Param 4: to (address, 32 bytes with padding)
Param 5: deadline (uint256, 32 bytes)

Full calldata: 0x38ed1739 + concatenated params
```

#### Step 3: Structure YAML Template

```yaml
transactions:
  # Setup: Approve router to spend Token0
  - type: eip1559
    to: "0xTokenAddress"
    value: "0.0"
    gas_limit: 100000
    max_fee_per_gas: "5"
    max_priority_fee_per_gas: "2"
    input: "0x095ea7b3<router_address_padded><max_uint256>"

  # Swap: Token0 → Token1
  - type: eip1559
    to: "0xRouterAddress"
    value: "0.0"
    gas_limit: 200000
    max_fee_per_gas: "5"
    max_priority_fee_per_gas: "2"
    input: "0x38ed1739<encoded_params>"

  # Reverse Swap: Token1 → Token0
  - type: eip1559
    to: "0xRouterAddress"
    value: "0.0"
    gas_limit: 200000
    max_fee_per_gas: "5"
    max_priority_fee_per_gas: "2"
    input: "0x38ed1739<encoded_params_reversed>"
```

#### Step 4: Add Documentation Comments

```yaml
# Uniswap V2 Trading Simulation
#
# Contract Addresses:
#   Router: 0x...
#   Token0: 0x...
#   Token1: 0x...
#
# Prerequisites:
#   1. Approve Router to spend Token0 and Token1
#   2. Ensure signer has sufficient token balances
#   3. Verify liquidity exists in the pair
#
# Transaction Flow:
#   - Approve (runs once, subsequent calls succeed but waste gas)
#   - Swap Token0 for Token1
#   - Swap Token1 back to Token0
#   This creates continuous trading volume

transactions:
  # ... template content ...
```

#### Step 5: Test Template

**Initial Test - Single Transaction:**
```bash
cargo run --bin malachitebft-eth-utils -- spam \
  --dex \
  --template my_protocol_template.yaml \
  --rate 1 \
  --num-txs 1 \
  --rpc-url 127.0.0.1:8545
```

**Verify:**
- Transaction succeeds
- Gas usage is reasonable
- Expected state changes occur

**Full Test - Multiple Transactions:**
```bash
cargo run --bin malachitebft-eth-utils -- spam \
  --dex \
  --template my_protocol_template.yaml \
  --rate 10 \
  --num-txs 100 \
  --rpc-url 127.0.0.1:8545
```

**Monitor:**
- Success rate
- Error types
- Nonce synchronization
- Protocol state consistency

---

## Common Patterns

### Pattern 1: Approval + Operation
```yaml
transactions:
  # 1. One-time setup (approval)
  - type: eip1559
    to: "0xTokenAddress"
    input: "0x095ea7b3..."  # approve(spender, amount)

  # 2. Repeated operations
  - type: eip1559
    to: "0xProtocolAddress"
    input: "0x..."  # operation that spends tokens
```

### Pattern 2: Symmetric Trading
```yaml
transactions:
  # 1. Buy/Sell
  - type: eip1559
    to: "0xDEXAddress"
    input: "0x..."  # newSellOrder()

  # 2. Matching Buy
  - type: eip1559
    to: "0xDEXAddress"
    input: "0x..."  # newBuyOrder()
```

### Pattern 3: State Initialization + Operations
```yaml
transactions:
  # 1. Initialize state (fails after first success)
  - type: eip1559
    to: "0xProtocolAddress"
    input: "0x..."  # initState()

  # 2. Operations requiring initialized state
  - type: eip1559
    to: "0xProtocolAddress"
    input: "0x..."  # doSomething()
```

### Pattern 4: Multi-Step Workflow
```yaml
transactions:
  # 1. Wrap ETH
  - type: eip1559
    to: "0xWETHAddress"
    value: "0.1"
    input: "0xd0e30db0"  # deposit()

  # 2. Add liquidity
  - type: eip1559
    to: "0xPoolAddress"
    input: "0x..."  # addLiquidity()

  # 3. Stake LP tokens
  - type: eip1559
    to: "0xStakingAddress"
    input: "0x..."  # stake()
```

---

## Advanced Features

### Nonce Management
The spammer automatically:
- Fetches latest nonce from node on start
- Increments nonce for each transaction
- Spawns background tasks to verify nonce consistency
- Re-syncs if nonce mismatch detected (e.g., from RPC issues)

**Nonce Verification Flow:**
```rust
// After sending batch
1. Send transactions with nonces N, N+1, N+2, ...
2. Spawn background task to check pending nonce
3. If actual_nonce != expected_nonce:
   - Log warning
   - Re-sync to actual nonce
4. Continue with corrected nonce
```

### Batch RPC Requests
Transactions are sent in batches per second:
- Prepare all transactions for the interval
- Sign each transaction
- Send as single JSON-RPC batch request
- Process results individually

**Benefits:**
- Reduced network overhead
- Higher throughput
- Atomic rate limiting per interval

### Statistics Tracking
Two-level statistics:
- **Per-second stats:** Reset every second, logged incrementally
- **Total stats:** Cumulative across entire run

**Metrics:**
- Successful transactions
- Total bytes sent
- Failed transactions by error type
- Txpool status (pending/queued counts)

---

## Troubleshooting

### Issue: "Price does not match the index"
**Cause:** Protocol state doesn't match template assumptions

**Solution:**
1. Verify protocol state matches template expectations
2. Update `priceIdx` or similar parameters
3. Add initialization transactions to template

### Issue: "ERC20: insufficient allowance"
**Cause:** Missing or expired token approvals

**Solution:**
1. Run approval transactions before spamming
2. Include approval transactions at start of template
3. Use max uint256 for unlimited approval

### Issue: "Nonce mismatch detected"
**Cause:** RPC nonce inconsistency or transaction failures

**Solution:**
- Spammer automatically re-syncs
- If persistent, check RPC node health
- Verify transactions are being mined

### Issue: High failure rate
**Causes:**
- Insufficient gas limits
- Wrong parameter values
- Missing prerequisites
- Protocol state changes during run

**Solutions:**
1. Test single transaction first
2. Verify all prerequisites
3. Increase gas limits
4. Check protocol state consistency
5. Review transaction logs in `utils/logs/`

### Issue: Template not loading
**Causes:**
- YAML syntax errors
- Wrong file path
- Missing `--dex` flag

**Solutions:**
1. Validate YAML syntax
2. Use absolute or correct relative path
3. Ensure `--dex` flag is set
4. Check file permissions

---

## Best Practices

### 1. Start Small
- Test with `--rate 1 --num-txs 1` first
- Verify single transaction success
- Gradually increase rate and count

### 2. Document Your Templates
- Include contract addresses in comments
- Explain each transaction's purpose
- List prerequisites clearly
- Document expected behavior

### 3. Use Realistic Parameters
- Match production-like amounts
- Use appropriate gas limits
- Consider network congestion

### 4. Monitor During Execution
- Watch log output for errors
- Check txpool status
- Verify protocol state changes

### 5. Handle Setup Transactions
- Include approvals if needed
- Initialize protocol state if required
- Document which transactions may fail on repeat

### 6. Version Control Templates
- Track template changes
- Document protocol version compatibility
- Note when addresses change (testnets, deployments)

---

## Template Validation Checklist

Before running a new template:

- [ ] All addresses are correct (checksummed format)
- [ ] Function selectors match contract ABI
- [ ] Parameter encoding is correct
- [ ] Gas limits are sufficient
- [ ] Prerequisites are documented
- [ ] Token approvals are handled
- [ ] Single transaction test passes
- [ ] Expected state changes occur
- [ ] Error handling is appropriate
- [ ] Comments explain transaction flow

---

## Information Template for New Protocols

Use this template when requesting a spammer template for a new protocol:

```markdown
## Protocol Information

**Protocol Name:** [e.g., Uniswap V3]
**Network:** [e.g., Ethereum Mainnet, Testnet]

### Contracts
- Contract 1 Name: [Address]
- Contract 2 Name: [Address]

### Tokens (if applicable)
- Token A: [Address, Decimals]
- Token B: [Address, Decimals]

### Functions to Call
1. Function Name: `functionSignature(type1,type2)`
   - Parameter 1: [Description, valid range]
   - Parameter 2: [Description, valid range]
   - Expected behavior: [What it does]
   - Gas estimate: [Typical gas used]

2. [Additional functions...]

### Prerequisites
- [ ] Prerequisite 1 (e.g., Token approvals)
- [ ] Prerequisite 2 (e.g., Minimum balance)

### Desired Transaction Flow
1. [First transaction type and purpose]
2. [Second transaction type and purpose]
3. [Continue pattern...]

### Testing Parameters
- Transaction rate: [e.g., 10 tx/s]
- Total transactions: [e.g., 1000]
- Test duration: [e.g., 100 seconds]

### Special Considerations
- [Any protocol-specific quirks]
- [State dependencies]
- [Known failure modes]
```

---

## Summary

Spammer templates enable:
- **Automated Protocol Testing:** Simulate real usage patterns
- **High-Throughput Stress Testing:** Push protocol and network limits
- **Flexible Configuration:** YAML-based, easy to modify
- **Round-Robin Execution:** Realistic transaction variety
- **Comprehensive Monitoring:** Detailed logging and statistics

**To create a template for a new protocol, provide:**
1. Contract addresses
2. Function signatures and parameters
3. Token information (if applicable)
4. Expected transaction flow
5. Prerequisites and setup steps
6. Gas requirements

With this information, a complete spammer template can be generated following the patterns and structure documented here.
