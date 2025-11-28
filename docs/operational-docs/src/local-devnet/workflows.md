# Common Workflows

Here are some typical workflows for using the local testnet during development.

## Smart Contract Deployment

1. Start the network:
   ```bash
   make
   ```

2. Deploy your contract using Foundry:
   ```bash
   forge create src/MyContract.sol:MyContract \
     --rpc-url http://127.0.0.1:8545 \
     --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
   ```

3. Verify in Otterscan:
   - Open http://localhost:5100
   - Search for the contract address
   - View deployment transaction and contract state

4. Interact with the contract:
   ```bash
   cast call <CONTRACT_ADDRESS> "myFunction()" --rpc-url http://127.0.0.1:8545
   ```

## Validator Set Changes

1. Start the network:
   ```bash
   make
   ```

2. Check initial validator set:
   ```bash
   cargo run --bin emerald-utils poa list
   ```

3. Create a new validator key:
   ```bash
   cargo run --bin emerald -- init --home nodes/new_validator
   ```

4. Get the public key:
   ```bash
   cargo run --bin emerald show-pubkey nodes/new_validator/config/priv_validator_key.json
   ```

5. Add the validator to the network:
   ```bash
   cargo run --bin emerald-utils poa add-validator \
     --validator-pubkey <PUBKEY> \
     --power 100 \
     --owner-private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
   ```

6. Verify the change:
   ```bash
   cargo run --bin emerald-utils poa list
   ```

7. Start the new validator node (manual process, see node configuration)

## Testing Under Load

1. Start the network:
   ```bash
   make
   ```

2. Run the transaction spammer (if available in your repo):
   ```bash
   cargo run --bin tx-spammer -- \
     --rpc-url http://127.0.0.1:8545 \
     --rate 10 \
     --duration 60
   ```

3. Monitor performance in Grafana:
   - Open http://localhost:3000
   - Watch block production rate
   - Monitor transaction processing time
   - Check for any consensus delays

4. Check mempool and logs:
   ```bash
   # Check mempool size
   curl -X POST http://127.0.0.1:8545 \
     -H "Content-Type: application/json" \
     -d '{"jsonrpc":"2.0","method":"txpool_status","params":[],"id":1}'

   # Watch validator logs
   tail -f nodes/0/emerald.log
   ```

## Iterative Development

When you need a clean state:

1. Stop and clean:
   ```bash
   make clean
   ```

2. Restart fresh:
   ```bash
   make
   ```

3. Redeploy contracts and test again

Tip: This is faster than manually resetting the blockchain state and ensures a consistent starting point.

## Application Integration

1. Start the network:
   ```bash
   make
   ```

2. Configure your application to use:
   - RPC URL: `http://127.0.0.1:8545`
   - Chain ID: `12345`
   - Test account private key (from pre-funded accounts)

3. Run your application and verify:
   - Transactions are submitted successfully
   - Events are emitted and captured correctly
   - State changes are reflected

4. Use Otterscan to debug any issues:
   - View transaction details
   - Check revert reasons
   - Inspect logs and events
