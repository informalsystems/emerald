# ValidatorManager Contract Upgrades

The `ValidatorManager` contract is deployed behind a UUPS (Universal Upgradeable Proxy Standard) proxy at the predefined address `0x0000000000000000000000000000000000002000`. This allows the contract logic to be upgraded without changing the address that validators, the consensus engine, and tooling interact with.

## Architecture

```
Callers (Rust app, CLI, etc.)
        |
        v
  ERC1967Proxy at 0x2000        <-- permanent address, holds all storage
        |  delegatecall
        v
  ValidatorManager impl at 0x2001  <-- replaceable logic contract
```

- The **proxy** at `0x2000` holds all storage (validators, owner, total power) and delegates every call to the current implementation.
- The **implementation** at `0x2001` (at genesis) contains only the contract logic. It can be replaced by deploying a new implementation and calling `upgradeToAndCall` on the proxy.
- Only the contract **owner** can authorize upgrades.

## How to Upgrade

### 1. Prepare the New Implementation

Write the updated `ValidatorManager` contract. The new version **must**:

- Inherit from the same base contracts (`Initializable`, `OwnableUpgradeable`, `ReentrancyGuardUpgradeable`, `UUPSUpgradeable`)
- Keep all existing state variables in the same order
- Only append new state variables at the end

### 2. Deploy the New Implementation

Deploy the new implementation contract to any address using a standard transaction:

```bash
forge create --rpc-url <RPC_URL> --private-key <DEPLOYER_KEY> src/ValidatorManager.sol:ValidatorManager
```

Note the deployed address (e.g. `0xNewImplAddress`).

### 3. Call `upgradeToAndCall` on the Proxy

As the contract owner, call the upgrade function on the proxy address (`0x2000`):

```bash
cast send 0x0000000000000000000000000000000000002000 \
  "upgradeToAndCall(address,bytes)" \
  <NEW_IMPL_ADDRESS> \
  "0x" \
  --rpc-url <RPC_URL> \
  --private-key <OWNER_KEY>
```

If the new version needs migration logic, encode a `reinitializer(n)` call as the second argument instead of `"0x"`.

### 4. Verify

```bash
# Check the implementation address changed
cast storage 0x0000000000000000000000000000000000002000 \
  0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc \
  --rpc-url <RPC_URL>

# Verify existing state is intact
cast call 0x0000000000000000000000000000000000002000 \
  "getValidatorCount()(uint256)" \
  --rpc-url <RPC_URL>
```

## Storage Layout Rules

The proxy holds all storage. Upgrades must preserve storage compatibility:

| Rule | Detail |
|------|--------|
| Never reorder state variables | Changing the order shifts all subsequent slots, corrupting data |
| Never remove state variables | Removing a variable shifts all subsequent slots |
| Never insert state variables in the middle | Same as reorder -- shifts slots |
| Only append new variables at the end | New variables get fresh sequential slots after existing ones |
| Do not change variable types to different sizes | e.g. changing `uint64` to `uint256` changes the slot packing |

OpenZeppelin base contracts (`OwnableUpgradeable`, etc.) use ERC-7201 namespaced storage, so their internal storage will not collide with the contract's sequential slots.

## Using `reinitializer` for Migration Logic

If an upgrade needs to run one-time migration logic (e.g. initializing a new state variable), use the `reinitializer` modifier:

```solidity
function initializeV2(uint256 newParam) public reinitializer(2) {
    _newStateVar = newParam;
}
```

Then pass the encoded call as the second argument to `upgradeToAndCall`:

```bash
cast send 0x0000000000000000000000000000000000002000 \
  "upgradeToAndCall(address,bytes)" \
  <NEW_IMPL_ADDRESS> \
  $(cast calldata "initializeV2(uint256)" 42) \
  --rpc-url <RPC_URL> \
  --private-key <OWNER_KEY>
```

The `reinitializer(n)` modifier ensures the migration can only run once and in sequence (version 2 after version 1, etc.).

## Limitations

- **Proxy address is permanent**: `0x2000` can never change. All callers reference this address.
- **Constructor logic does not run on upgrade**: Use `reinitializer(n)` for any migration logic needed by a new version.
- **Owner key compromise is critical**: The owner can upgrade to an arbitrary implementation, effectively taking full control. Protect the owner key accordingly.
- **`renounceOwnership` permanently locks upgrades**: Once ownership is renounced, no further upgrades are possible. This is irreversible.
- **Existing chains migration**: Chains already running with the non-upgradeable contract require a hard fork to migrate to the proxy pattern. This involves replacing the code at `0x2000` with proxy bytecode, deploying the implementation at a new address, and adjusting storage layout (the slot positions change between the non-upgradeable and upgradeable versions).

## Pre-Upgrade Checklist

1. Verify storage compatibility: all existing state variables are in the same position
2. Run `forge test` against the new implementation with upgrade tests
3. Test the upgrade on a local devnet first
4. Audit the new implementation for correctness and security
5. Verify the owner key is available and functional
6. Communicate the upgrade plan to all network participants

## Genesis Deployment Details

At genesis, the proxy and implementation are deployed via alloc (no constructor execution):

- **`0x2000`**: `ValidatorManagerProxy` runtime bytecode + pre-computed storage (EIP-1967 implementation slot, ERC-7201 Ownable/ReentrancyGuard/Initializable slots, validator state)
- **`0x2001`**: `ValidatorManager` runtime bytecode + Initializable storage set to `type(uint64).max` (locks the implementation against direct initialization, since the constructor's `_disableInitializers()` doesn't run during genesis alloc)
