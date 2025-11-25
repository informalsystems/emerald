# Troubleshooting

## Network Won't Start

1. Check if ports are in use

   ```bash
   lsof -i :8545  # RPC port
   lsof -i :30303 # P2P port
   ```

2. View Docker logs

   ```bash
   docker compose logs reth0
   docker compose logs reth1
   ```

3. Verify genesis file exists

   ```bash
   ls -la assets/genesis.json
   ```

4. Check emerald logs
   ```bash
   tail -f nodes/0/emerald.log
   ```

## Validator Operations Fail

1. Verify network is running

   ```bash
   curl -X POST http://127.0.0.1:8545 \
     -H "Content-Type: application/json" \
     -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
   ```

2. Check validator public key format
   - Must be hex-encoded secp256k1 public key
   - Can be 64 bytes (raw) or 65 bytes (with `0x04` prefix)
   - Include `0x` prefix

3. Verify contract owner key
   - Default: `0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80`

## Public Key Extraction

To get a validator's public key from their private key file:

```bash
cargo run --bin emerald show-pubkey \
  nodes/0/config/priv_validator_key.json
```
