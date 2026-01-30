# Creating Network Genesis

This section covers the key exchange process between you (the network coordinator) and the network validators.

## Step 1: Instruct Validators to Generate Keys

As the network coordinator, you need to provide each validator with the following instructions to generate their validator keys **on their own infrastructure**.

### Instructions to Send to Validators:

---

**Validator Key Generation Instructions**

To participate in the network, you need to generate your validator signing keys. Follow these steps:

1. **Install Emerald** (if not already installed):
   ```bash
   git clone https://github.com/informalsystems/emerald.git
   cd emerald
   cargo build --release
   ```

This will build the Emerald binary and place it under `target/release/custom-reth` which can then be copied to the desired machine under `/usr/local/bin/custom-reth` for example.

2. **Generate your validator private key**:
   ```bash
   emerald init --home /path/to/home_dir
   ```

   This creates a private key file at `<home_dir>/config/priv_validator_key.json`

   > [!IMPORTANT]
   > Keep this file secure and private. 
   > Never share this file with anyone, including the network coordinator.

3. **Extract your public key**:
   ```bash
   emerald show-pubkey <home_dir>/config/priv_validator_key.json
   ```

   This will output a public key string like:
   ```
   0xd8620dd478f043bd27fc9389ec6873410265cf8640cb636decd2f0a2ddad7aa5656e58f05b1596a9c737f7073211089c6b49ab7ad5bdb9ab55bf83741b3ee4e4
   ```

4. **Provide your public key to the network coordinator**: Send only this public key string (starting with `0x`) to the network coordinator. Do not send your private key file.

---

## Step 2: Collect Public Keys from Validators

Once validators have generated their keys, collect all the public keys they provide. You should receive one public key per validator, each looking like:

```
0xd8620dd478f043bd27fc9389ec6873410265cf8640cb636decd2f0a2ddad7aa5656e58f05b1596a9c737f7073211089c6b49ab7ad5bdb9ab55bf83741b3ee4e4
```

Create a file (e.g., `validator_public_keys.txt`) with one public key per line:

```
0xd8620dd478f043bd27fc9389ec6873410265cf8640cb636decd2f0a2ddad7aa5656e58f05b1596a9c737f7073211089c6b49ab7ad5bdb9ab55bf83741b3ee4e4
0x9b9fc5d66ec179df923dfbb083f2e846ff5da508650c77473c8427fafe481a5e73c1ad26bed12895108f463b84f6dd0d8ebbf4270a06e312a3b63295cffebbff
0x317052004566d1d2ac0b3161313646412f93275599eb6455302a050352027905346eb4a0eebce874c35b1cd29bb5472c46eb2fd9ed24e57c2b73b85b59729e36
```

## Step 3: Setup PoA Address

As the network coordinator, you need to create a _PoA admin key_ that will control validator set management (adding, removing, and updating validators).

Use your preferred Ethereum key management tool (e.g., MetaMask, cast, or any Ethereum wallet) to generate a new private key. You will need the **address** (e.g., `0x123abc...`) for the next step.

> [!IMPORTANT]
> This PoA address will have authority over the validator set, so keep the private key secure.

## Step 4: Generate Genesis Files

Now that you have collected all validator public keys and have your PoA address, you can generate the genesis files for both Reth and Emerald.

Run the following command with your `validator_public_keys.txt` file:

```
emerald genesis \
  --public-keys-file /path/to/validator_public_keys.txt \
  --chain-id 12345 \
  --poa-owner-address <ADDRESS_GENERATED_IN_PREVIOUS_STEP> \
  --evm-genesis-output ./eth-genesis.json \
  --emerald-genesis-output ./emerald-genesis.json
```

This command takes all the validator public keys and generates:
- **`eth-genesis.json`**: Genesis file for Reth (execution layer), including the PoA smart contract
- **`emerald-genesis.json`**: Genesis file for Emerald (consensus layer)

## Step 5: Distribute Genesis Files to Validators

Now you need to share the generated genesis files with all validator participants:

1. **Send the genesis files**: Provide both `eth-genesis.json` and `emerald-genesis.json` to each validator
2. **Share network parameters**: Include the following information:
   - Chain ID (the value you used in the genesis command)
   - JWT secret (which you'll generate in the next section - all nodes must use the same JWT)
   - Peer connection details (IP addresses and ports for other validators)
3. **Coordinate node configurations**: Each validator will need to configure their Reth and Emerald nodes (see sections below)

> [!IMPORTANT]
> All nodes in the network must use the **same** genesis files. 
> Any difference will result in nodes being unable to reach consensus.