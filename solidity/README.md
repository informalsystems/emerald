## Foundry

**Foundry is a blazing fast, portable and modular toolkit for Ethereum application development written in Rust.**

Foundry consists of:

- **Forge**: Ethereum testing framework (like Truffle, Hardhat and DappTools).
- **Cast**: Swiss army knife for interacting with EVM smart contracts, sending transactions and getting chain data.
- **Anvil**: Local Ethereum node, akin to Ganache, Hardhat Network.
- **Chisel**: Fast, utilitarian, and verbose solidity REPL.

## Documentation

https://book.getfoundry.sh/

## Usage

### Build

```shell
$ forge build
```

### Test

```shell
$ forge test
```

### Format

```shell
$ forge fmt
```

### Gas Snapshots

```shell
$ forge snapshot
```

### Anvil

```shell
$ anvil
```

### Deploy

```shell
$ forge script script/Counter.s.sol:CounterScript --rpc-url <your_rpc_url> --private-key <your_private_key>
```

### Cast

```shell
$ cast <subcommand>
```

### Help

```shell
$ forge --help
$ anvil --help
$ cast --help
```

# Example

This example was generated using the [OpenZeppelin Wizard](https://wizard.openzeppelin.com/) for ERC20. It includes mintable and burnable features.

## Deploy the Contract

```bash
forge script script/DeployToken.s.sol \
    --rpc-url http://localhost:8645 \
    --private-key YOUR_PRIVATE_KEY \
    --broadcast
```

## Interacting with Example

### Mint Tokens

```bash
# Mint 1000 tokens to your address
cast send CONTRACT_ADDRESS "mint(address,uint256)" YOUR_ADDRESS 1000000000000000000000 \
    --private-key YOUR_PRIVATE_KEY \
    --rpc-url http://localhost:8645

# Check your balance
cast call CONTRACT_ADDRESS "balanceOf(address)" YOUR_ADDRESS --rpc-url http://localhost:8645
```

### Transfer Tokens

```bash
# Transfer 100 tokens to another address
cast send CONTRACT_ADDRESS "transfer(address,uint256)" RECIPIENT_ADDRESS 100000000000000000000 \
    --private-key YOUR_PRIVATE_KEY \
    --rpc-url http://localhost:8645
```

### Burn Tokens

```bash
# Burn 50 tokens from your balance
cast send CONTRACT_ADDRESS "burn(uint256)" 50000000000000000000 \
    --private-key YOUR_PRIVATE_KEY \
    --rpc-url http://localhost:8645
```

### Check Total Supply

```bash
# See how many tokens exist in total
cast call CONTRACT_ADDRESS "totalSupply()" --rpc-url http://localhost:8645
```
