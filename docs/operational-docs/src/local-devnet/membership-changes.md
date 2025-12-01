# Managing Validators

Once the network is running, you can dynamically manage the validator set by adding, removing, or updating validators without restarting the network.

## PoA Module

Emerald uses a Proof of Authority (PoA) smart contract (`ValidatorManager`) to manage the validator set. This contract is deployed at a predefined address (`0x0000000000000000000000000000000000002000`) and controls:

- Which validators are active
- Each validator's voting power
- Who can modify the validator set (the contract owner)

Emerald's PoA tooling provides support for the following use cases. 

- **Testing validator changes.** Simulate adding/removing validators in a running network
- **Testing voting power.** Experiment with different power distributions
- **Integration testing.** Test how your application handles validator set changes
- **Learning.** Understand how dynamic validator management works

## Prerequisites

- Running testnet (see [Create a New Network](create-network.md))
- RPC endpoint (default `http://127.0.0.1:8645`)
- Contract owner key (see below for default test key)

## Testnet Accounts

The local testnet uses a well-known test mnemonic for pre-funded accounts.

**Mnemonic**: `test test test test test test test test test test test junk`

**PoA Contract Owner (Account #0)**:
- **Private Key**: `0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80`
- **Address**: `0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266`
- **Role**: Has authority to add/remove/update validators

**Validator Keys**:
- Located at `nodes/{0,1,2,3}/config/priv_validator_key.json`
- These are separate from the Ethereum accounts
- Used for consensus signing, not transactions

**Important**: These keys are for **testing only**. Never use them on public networks or with real funds.

## List Current Validators

View all registered validators and their voting power:

```bash
cargo run --bin emerald-utils poa list
```

Output:

```
Total validators: 4

Validator #1:
  Power: 100
  Pubkey: 04681eaaa34e491e6c8335abc9ea92b024ef52eb91442ca3b84598c79a79f31b75...
  Validator address: 0x1234567890abcdef...

Validator #2:
  Power: 100
  ...
```

## Add a Validator

To add a node to the validator set, you need the node's public key. There are two options:

- Use one of the existing validators after [removing](#remove-a-validator) it from the validator set.

- Add a new node using the following command:
  ```bash
  # replace ID with a specific node ID (e.g., 4)
  cargo run --bin emerald -- init --home nodes/{ID}
  ```

To get the node's public key, run the following command.

```bash
# replace ID with a specific node ID (e.g., 4)
cargo run --bin emerald show-pubkey \
  nodes/{ID}/config/priv_validator_key.json
```

Then run the following command, replacing the placeholder values.

```bash
cargo run --bin emerald-utils poa add-validator \
  --validator-pubkey <PUBKEY> \
  --power 100 \
  --owner-private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```

Parameters:

- `--validator-pubkey`: Uncompressed secp256k1 public key (65 bytes with `0x04` prefix, or 64 bytes raw)
- `--power`: Voting weight (default: 100)
- `--owner-private-key`: Private key of the ValidatorManager contract owner

Optional flags:

- `--rpc-url`: RPC endpoint (default: `http://127.0.0.1:8645`)
- `--contract-address`: ValidatorManager address (default: `0x0000000000000000000000000000000000002000`)

## Remove a Validator

To remove a validator from the active set:

```bash
cargo run --bin emerald-utils poa remove-validator \
  --validator-pubkey 0x04681eaaa34e491e6c8335abc9ea92b024ef52eb91442ca3b84598c79a79f31b75... \
  --owner-private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```

## Update Validator Power

To change a validator's voting weight:

```bash
cargo run --bin emerald-utils poa update-validator \
  --validator-pubkey 0x04681eaaa34e491e6c8335abc9ea92b024ef52eb91442ca3b84598c79a79f31b75... \
  --power 200 \
  --owner-private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```