# Testnet commands

## Starting the testnet

The command used to start the local testnet is the following:

<details>
<summary><code>emerald testnet start</code></summary>

```shell
{{#include ../templates/help_templates/testnet/start.md}}
```
</details>

Start a testnet with 4 Reth nodes and 4 Emerald nodes.

<details>
<summary>Output for <code>emerald testnet start -n 4</code></summary>

```🚀 Initializing testnet with 4 nodes...

Checking custom-reth installation... ✓ Reth Version: 1.9.2-dev

📝 Generating testnet configuration...
2025-12-02T10:10:54.667684Z  INFO Generating configuration for node... id=0 home=$HOME/.emerald-devnet/0 emerald_config=$HOME/.emerald-devnet/0/config/emerald.toml
2025-12-02T10:10:54.668378Z  INFO Generating configuration for node... id=1 home=$HOME/.emerald-devnet/1 emerald_config=$HOME/.emerald-devnet/1/config/emerald.toml
2025-12-02T10:10:54.668769Z  INFO Generating configuration for node... id=2 home=$HOME/.emerald-devnet/2 emerald_config=$HOME/.emerald-devnet/2/config/emerald.toml
2025-12-02T10:10:54.669118Z  INFO Generating configuration for node... id=3 home=$HOME/.emerald-devnet/3 emerald_config=$HOME/.emerald-devnet/3/config/emerald.toml
✓ Configuration generated

📦 Setting up assets directory...
✓ Assets directory set up

⚙️  Generating Emerald configs...
✓ Emerald configs generated

🔑 Extracting validator public keys...
2025-12-02T10:10:54.669860Z  INFO Using `./target/debug/emerald` for Emerald binary when extracting public keys
2025-12-02T10:10:54.676766Z  INFO Using `./target/debug/emerald` for Emerald binary when extracting public keys
2025-12-02T10:10:54.682883Z  INFO Using `./target/debug/emerald` for Emerald binary when extracting public keys
2025-12-02T10:10:54.689414Z  INFO Using `./target/debug/emerald` for Emerald binary when extracting public keys
✓ Public keys extracted

⚙️  Generating genesis file...
  Using emerald-utils from: ./target/debug/emerald-utils
✓ Genesis file created

🔗 Starting Reth execution clients...
  Starting Reth node 0... Starting Reth node 0 on ports:
  HTTP: 8645
  AuthRPC: 8647
  Metrics: 8648
  P2P: 8649
  Logs: $HOME/.emerald-devnet/0/logs/reth.log
✓ (PID: 64655)
  Starting Reth node 1... Starting Reth node 1 on ports:
  HTTP: 8675
  AuthRPC: 8677
  Metrics: 8678
  P2P: 8679
  Logs: $HOME/.emerald-devnet/1/logs/reth.log
✓ (PID: 64678)
  Starting Reth node 2... Starting Reth node 2 on ports:
  HTTP: 8705
  AuthRPC: 8707
  Metrics: 8708
  P2P: 8709
  Logs: $HOME/.emerald-devnet/2/logs/reth.log
✓ (PID: 64693)
  Starting Reth node 3... Starting Reth node 3 on ports:
  HTTP: 8735
  AuthRPC: 8737
  Metrics: 8738
  P2P: 8739
  Logs: $HOME/.emerald-devnet/3/logs/reth.log
✓ (PID: 64706)
✓ All Reth nodes started

⏳ Waiting for Reth nodes to initialize...
  Waiting for Reth node 0 to be ready... ✓
  Waiting for Reth node 1 to be ready... ✓
  Waiting for Reth node 2 to be ready... ✓
  Waiting for Reth node 3 to be ready... ✓
✓ All Reth nodes ready

🔗 Connecting Reth peers...
  Getting enode for Reth node 0... ✓
  Getting enode for Reth node 1... ✓
  Getting enode for Reth node 2... ✓
  Getting enode for Reth node 3... ✓
  Connecting node 0 -> 1... ✓
  Connecting node 0 -> 2... ✓
  Connecting node 0 -> 3... ✓
  Connecting node 1 -> 0... ✓
  Connecting node 1 -> 2... ✓
  Connecting node 1 -> 3... ✓
  Connecting node 2 -> 0... ✓
  Connecting node 2 -> 1... ✓
  Connecting node 2 -> 3... ✓
  Connecting node 3 -> 0... ✓
  Connecting node 3 -> 1... ✓
  Connecting node 3 -> 2... ✓
✓ Reth peers connected

💎 Starting Emerald consensus nodes...
  Starting Emerald node 0... 2025-12-02T10:10:57.694008Z  INFO Using `./target/debug/emerald` for Emerald binary to spawn node
✓ (PID: 64731)
  Starting Emerald node 1... 2025-12-02T10:10:58.303003Z  INFO Using `./target/debug/emerald` for Emerald binary to spawn node
✓ (PID: 64744)
  Starting Emerald node 2... 2025-12-02T10:10:58.914034Z  INFO Using `./target/debug/emerald` for Emerald binary to spawn node
✓ (PID: 64757)
  Starting Emerald node 3... 2025-12-02T10:10:59.520426Z  INFO Using `./target/debug/emerald` for Emerald binary to spawn node
✓ (PID: 64780)
✓ All Emerald nodes started

✅ Testnet started successfully!

📊 Status:
  Reth processes: 4 running
  Emerald processes: 4 running

📁 Logs:
  Reth: $HOME/.emerald-devnet/{0..3}/logs/reth.log
  Emerald: $HOME/.emerald-devnet/{0..3}/logs/emerald.log

💡 Commands:
    emerald testnet status           - Check status of all nodes
    emerald testnet stop-node <id>   - Stop a specific node
    emerald testnet stop             - Stop all nodes
    emerald testnet destroy          - Remove all testnet data
```
</details>

## Checking the status of the testnet

The status of the testnet can be displayed with the following:

<details>
<summary><code>emerald testnet status</code></summary>

```shell
{{#include ../templates/help_templates/testnet/status.md}}
```
</details>

Running the status after having started the testnet should give the following output:


<details>
<summary>Output for <code>emerald testnet status</code></summary>

```
📊 Testnet Status
Looking for nodes in: $HOME/.emerald-devnet

Node 0:
  Emerald: Running (PID: 64731)
  Reth:    Running (PID: 64655)
  Height:  51
  Peers:   3

Node 1:
  Emerald: Running (PID: 64744)
  Reth:    Running (PID: 64678)
  Height:  51
  Peers:   3

Node 2:
  Emerald: Running (PID: 64757)
  Reth:    Running (PID: 64693)
  Height:  50
  Peers:   3

Node 3:
  Emerald: Running (PID: 64780)
  Reth:    Running (PID: 64706)
  Height:  51
  Peers:   3

Summary:
  Total nodes:    4
  Emerald running: 4/4
  Reth running:    4/4
```
</details>

## Stopping a node

The command used to stop a single node is the following:

<details>
<summary><code>emerald testnet stop-node $NODE_ID</code></summary>

```
{{#include ../templates/help_templates/testnet/stop-node.md}}
```
</details>

Running this command for Node `1` should output:
<details>
<summary>Output for <code>emerald stop-node 1</code></summary>

```
🛑 Stopping node 1...
  Stopping Reth process (PID: 64678)... ✓
  Stopping Emerald process (PID: 64744)... ✓

✅ Stopped 2 process(es) for node 1
```
</details>

And if the status should now output the following:
<details>
<summary>Output for <code>emerald status</code></summary>

```
📊 Testnet Status
Looking for nodes in: $HOME/.emerald-devnet

Node 0:
  Emerald: Running (PID: 64731)
  Reth:    Running (PID: 64655)
  Height:  173
  Peers:   2

Node 1:
  Emerald: Not started
  Reth:    Not started

Node 2:
  Emerald: Running (PID: 64757)
  Reth:    Running (PID: 64693)
  Height:  173
  Peers:   2

Node 3:
  Emerald: Running (PID: 64780)
  Reth:    Running (PID: 64706)
  Height:  173
  Peers:   2

Summary:
  Total nodes:    4
  Emerald running: 3/4
  Reth running:    3/4
```
</details>

## Restarting a node

The command used to restart an existing node is the following:

<details>
<summary><code>emerald testnet start-node $NODE_ID</code></summary>

```shell
{{#include ../templates/help_templates/testnet/start-node.md}}
```
</details>

The node which was previously sopped can be restored and should output the following:

<details>
<summary>Output for <code>emerald start-node 1</code></summary>

```
🚀 Starting node 1...
Checking custom-reth installation... ✓ Reth Version: 1.9.2-dev

🔗 Starting Reth execution client...
Starting Reth node 1 on ports:
  HTTP: 8675
  AuthRPC: 8677
  Metrics: 8678
  P2P: 8679
  Logs: $HOME/.emerald-devnet/1/logs/reth.log
✓ Reth node started (PID: 66177)

⏳ Waiting for Reth node to initialize...
✓ Reth node ready

🔗 Connecting to existing peers...
  Connecting to node 0... ✓
  Connecting to node 3... ✓
  Connecting to node 2... ✓
✓ Connected to peers

💎 Starting Emerald consensus node...
2025-12-02T10:13:12.137314Z  INFO Using `./target/debug/emerald` for Emerald binary to spawn node
✓ Emerald node started (PID: 66192)

✅ Node 1 started successfully!

📁 Logs:
  Reth: $HOME/.emerald-devnet/1/logs/reth.log
  Emerald: $HOME/.emerald-devnet/1/logs/emerald.log
```
</details>

And if the status should now output the following:
<details>
<summary>Output for <code>emerald status</code></summary>

```
📊 Testnet Status
Looking for nodes in: $HOME/.emerald-devnet

Node 0:
  Emerald: Running (PID: 64731)
  Reth:    Running (PID: 64655)
  Height:  197
  Peers:   3

Node 1:
  Emerald: Running (PID: 66192)
  Reth:    Running (PID: 66177)
  Height:  173
  Peers:   3

Node 2:
  Emerald: Running (PID: 64757)
  Reth:    Running (PID: 64693)
  Height:  197
  Peers:   3

Node 3:
  Emerald: Running (PID: 64780)
  Reth:    Running (PID: 64706)
  Height:  197
  Peers:   3

Summary:
  Total nodes:    4
  Emerald running: 4/4
  Reth running:    4/4
```
</details>

## Adding a node

The command used to add a non-validator node is the following:


<details>
<summary><code>emerald testnet add-node</code></summary>

```shell
{{#include ../templates/help_templates/testnet/add-node.md}}
```
</details>

Running this command should output:
<details>
<summary>Output for <code>emerald add-node</code></summary>

```
📝 Adding non-validator node to testnet...

Checking custom-reth installation... ✓ Reth Version: 1.9.2-dev

📋 Next available node ID: 4

📁 Creating node directories...
✓ Node directories created

📋 Copying genesis file...
✓ Genesis file copied

⚙️  Generating Malachite config...
✓ Malachite config generated

⚙️  Generating Emerald config...
✓ Emerald config generated

🔑 Generating private validator key...
2025-12-02T10:13:59.649684Z  INFO Using `./target/debug/emerald` for Emerald binary to generate private key
✓ Private validator key generated

🔗 Starting Reth execution client...
Starting Reth node 4 on ports:
  HTTP: 8765
  AuthRPC: 8767
  Metrics: 8768
  P2P: 8769
  Logs: $HOME/.emerald-devnet/4/logs/reth.log
✓ Reth node started (PID: 66756)

⏳ Waiting for Reth node to initialize...
✓ Reth node ready

🔗 Connecting to existing peers...
  Connecting to node 0... ✓
  Connecting to node 1... ✓
  Connecting to node 3... ✓
  Connecting to node 2... ✓
✓ Connected to peers

💎 Starting Emerald consensus node...
2025-12-02T10:14:00.785156Z  INFO Using `./target/debug/emerald` for Emerald binary when adding node
✓ Emerald node started (PID: 66771)

✅ Non-validator node 4 added successfully!

📁 Logs:
  Reth: $HOME/.emerald-devnet/4/logs/reth.log
  Emerald: $HOME/.emerald-devnet/4/logs/emerald.log
```
</details>

And if the status should now output the following:
<details>
<summary>Output for <code>emerald status</code></summary>

```
📊 Testnet Status
Looking for nodes in: $HOME/.emerald-devnet

Node 0:
  Emerald: Running (PID: 64731)
  Reth:    Running (PID: 64655)
  Height:  353
  Peers:   4

Node 1:
  Emerald: Running (PID: 66192)
  Reth:    Running (PID: 66177)
  Height:  353
  Peers:   4

Node 2:
  Emerald: Running (PID: 64757)
  Reth:    Running (PID: 64693)
  Height:  353
  Peers:   4

Node 3:
  Emerald: Running (PID: 64780)
  Reth:    Running (PID: 64706)
  Height:  353
  Peers:   4

Node 4:
  Emerald: Running (PID: 66771)
  Reth:    Running (PID: 66756)
  Height:  353
  Peers:   4

Summary:
  Total nodes:    5
  Emerald running: 5/5
  Reth running:    5/5
```
</details>

## Set the new node as validator

If we now look at the list of validator we should only see 4 as we previously added a non-validator node.
<details>
<summary>Output for <code>emerald-utils poa -r http://127.0.0.1:8645 list</code></summary>

```
POA Owner Address: 0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266

Total validators: 4

Validator #1:
  Power: 100
  Pubkey: 04d8620dd478f043bd27fc9389ec6873410265cf8640cb636decd2f0a2ddad7aa5656e58f05b1596a9c737f7073211089c6b49ab7ad5bdb9ab55bf83741b3ee4e4
Validator address: 0x5a9245dce516aa85c8d82a90608a542a151d9e91

Validator #2:
  Power: 100
  Pubkey: 049b9fc5d66ec179df923dfbb083f2e846ff5da508650c77473c8427fafe481a5e73c1ad26bed12895108f463b84f6dd0d8ebbf4270a06e312a3b63295cffebbff
Validator address: 0x7d17aa4fe6c1e7c58d1b26f5a68c35be0bff6c29

Validator #3:
  Power: 100
  Pubkey: 04317052004566d1d2ac0b3161313646412f93275599eb6455302a050352027905346eb4a0eebce874c35b1cd29bb5472c46eb2fd9ed24e57c2b73b85b59729e36
Validator address: 0x311e280d2918e93a90eea22c0773053f325ce409

Validator #4:
  Power: 100
  Pubkey: 049cdba83f09fd9f66cf5b45ce3db1866c85ce0041f0dcb3d64070196fc38690acc00c0dafa3289404b5615986e467720cf43ab970cc14c4f1f1a07774a992b3e0
Validator address: 0xe95eaa9dcd4f9e3b4eec820355c03b4f4499ab87
```
</details>

In order to add the new node as a validator first the public key needs to be retrieved.
<details>
<summary>Output for <code>emerald show-pubkey $HOME/.emerald-devnet/4/config/priv_validator_key.json</code></summary>

```
0x670252bba7f17bfa44ed4148aee562108a57f49e90017f940d80bd4a34e367710c192ed04ad87a71f6c3cff5d48b1baab8f423c01f534a01dee18b151b25a0f7
```
</details>

The validator can now be added.
<details>
<summary>Output for <code>
emerald-utils poa -r http://127.0.0.1:8645 add-validator \
  --validator-pubkey 0x670252bba7f17bfa44ed4148aee562108a57f49e90017f940d80bd4a34e367710c192ed04ad87a71f6c3cff5d48b1baab8f423c01f534a01dee18b151b25a0f7 \
  --power 100 \
  --owner-private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
</code></summary>  

```
Adding validator with pubkey: 0x670252bba7f17bfa44ed4148aee562108a57f49e90017f940d80bd4a34e367710c192ed04ad87a71f6c3cff5d48b1baab8f423c01f534a01dee18b151b25a0f7
  Power: 100
Transaction sent: 0xe1796369404585429fa24300d8f1f5433c8e5b477c992f1bd23e39d6c7de0ab6
Transaction confirmed in block: Some(466)
Gas used: 153301
```
</details>

And listing the validators should now output the following:

<details>
<summary>Output for <code>emerald-utils poa -r http://127.0.0.1:8645 list</code></summary>

```
POA Owner Address: 0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266

Total validators: 5

Validator #1:
  Power: 100
  Pubkey: 04d8620dd478f043bd27fc9389ec6873410265cf8640cb636decd2f0a2ddad7aa5656e58f05b1596a9c737f7073211089c6b49ab7ad5bdb9ab55bf83741b3ee4e4
Validator address: 0x5a9245dce516aa85c8d82a90608a542a151d9e91

Validator #2:
  Power: 100
  Pubkey: 049b9fc5d66ec179df923dfbb083f2e846ff5da508650c77473c8427fafe481a5e73c1ad26bed12895108f463b84f6dd0d8ebbf4270a06e312a3b63295cffebbff
Validator address: 0x7d17aa4fe6c1e7c58d1b26f5a68c35be0bff6c29

Validator #3:
  Power: 100
  Pubkey: 04317052004566d1d2ac0b3161313646412f93275599eb6455302a050352027905346eb4a0eebce874c35b1cd29bb5472c46eb2fd9ed24e57c2b73b85b59729e36
Validator address: 0x311e280d2918e93a90eea22c0773053f325ce409

Validator #4:
  Power: 100
  Pubkey: 049cdba83f09fd9f66cf5b45ce3db1866c85ce0041f0dcb3d64070196fc38690acc00c0dafa3289404b5615986e467720cf43ab970cc14c4f1f1a07774a992b3e0
Validator address: 0xe95eaa9dcd4f9e3b4eec820355c03b4f4499ab87

Validator #5:
  Power: 100
  Pubkey: 04670252bba7f17bfa44ed4148aee562108a57f49e90017f940d80bd4a34e367710c192ed04ad87a71f6c3cff5d48b1baab8f423c01f534a01dee18b151b25a0f7
Validator address: 0x42dccf7844765f8205edbe4364d69d955fd1330a
```
</details>

## Stopping the testnet

The command used to start the local testnet is the following:

<details>
<summary><code>emerald testnet stop</code></summary>

```shell
{{#include ../templates/help_templates/testnet/stop.md}}
```
</details>

The testnet can be stopped running this command and should output:

<details>
<summary>Output for <code>emerald testnet stop</code></summary>

```
🛑 Stopping all testnet nodes...

Stopping node 0...
  Stopping Reth (PID: 64655)... ✓
  Stopping Emerald (PID: 64731)... ✓
Stopping node 1...
  Stopping Reth (PID: 66177)... ✓
  Stopping Emerald (PID: 66192)... ✓
Stopping node 4...
  Stopping Reth (PID: 66756)... ✓
  Stopping Emerald (PID: 66771)... ✓
Stopping node 3...
  Stopping Reth (PID: 64706)... ✓
  Stopping Emerald (PID: 64780)... ✓
Stopping node 2...
  Stopping Reth (PID: 64693)... ✓
  Stopping Emerald (PID: 64757)... ✓

✅ Stopped 10/10 processes
```
</details>

## Destroying the testnet

The command used to remove all testnet data is the following:

<details>
<summary><code>emerald testnet destroy</code></summary>

```shell
{{#include ../templates/help_templates/testnet/destroy.md}}
```
</details>

The testnet data can now be removed:

<details>
<summary>Output for <code>emerald testnet destroy</code></summary>

```
⚠️  This will stop all nodes and permanently delete all testnet data at:
   $HOME/.emerald-devnet

   Are you sure? (y/N): y
🛑 Stopping all running nodes...

🗑️  Removing testnet data...
✅ Testnet data removed successfully
```
</details>