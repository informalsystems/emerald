# Interacting With the Testnet

Once your local Emerald testnet is running, you can interact with it like any Ethereum network.

## Using `curl` (JSON-RPC)

**Get current block number:**
```bash
curl -X POST http://127.0.0.1:8645 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
```

**Get account balance:**
```bash
curl -X POST http://127.0.0.1:8645 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_getBalance","params":["0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266","latest"],"id":1}'
```

## Using `cast` (Foundry)

**Prerequisite:** [Foundry](https://getfoundry.sh/introduction/installation/)

**Get block number:**
```bash
cast block-number --rpc-url http://127.0.0.1:8645
```

**Check balance:**
```bash
cast balance 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 --rpc-url http://127.0.0.1:8645
```

**Send ETH:**
```bash
cast send 0x70997970C51812dc3A010C7d01b50e0d17dc79C8 \
  --value 1ether \
  --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
  --rpc-url http://127.0.0.1:8645
```

## Using Web3 Libraries

Configure your Web3 library to connect to `http://127.0.0.1:8645`:

**ethers.js (JavaScript):**
```javascript
import { ethers } from 'ethers';

const provider = new ethers.JsonRpcProvider('http://127.0.0.1:8645');
const wallet = new ethers.Wallet('0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80', provider);

// Send transaction
const tx = await wallet.sendTransaction({
  to: '0x70997970C51812dc3A010C7d01b50e0d17dc79C8',
  value: ethers.parseEther('1.0')
});
await tx.wait();
```

**web3.py (Python):**
```python
from web3 import Web3

w3 = Web3(Web3.HTTPProvider('http://127.0.0.1:8645'))
account = w3.eth.account.from_key('0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80')

# Send transaction
tx = {
    'to': '0x70997970C51812dc3A010C7d01b50e0d17dc79C8',
    'value': w3.to_wei(1, 'ether'),
    'gas': 21000,
    'gasPrice': w3.eth.gas_price,
    'nonce': w3.eth.get_transaction_count(account.address),
}
signed_tx = account.sign_transaction(tx)
tx_hash = w3.eth.send_raw_transaction(signed_tx.rawTransaction)
```

## Using MetaMask

1. Open MetaMask and click on the network dropdown
2. Click "Add Network" → "Add a network manually"
3. Enter the following details:
   - **Network Name**: Emerald Local
   - **RPC URL**: `http://127.0.0.1:8645`
   - **Chain ID**: `12345` (or whatever you set in genesis)
   - **Currency Symbol**: ETH
4. Click "Save"
5. Import one of the test accounts using its private key

> [!WARNING]
> Only use test private keys with local networks. 
> _**Never import test keys into wallets used for real funds.**_