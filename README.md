
# TODO : Rename to Emerald

We need to rename the project to Emerald while clearly stating that it was built on top of
malaket-layered (circlefin/malaketh-layered). In addition to basic instructions, we should have a list
of features that were added on top of malaketh-layered.

# Malaketh-layered: Malachite for Ethereum execution clients via Engine API

Tendermint-based consensus engine for Ethereum execution clients, connected via [Engine API][engine-api].
Built as a shim layer on top of [Malachite][malachite].

**Table of contents**

- [Introduction](#introduction)
- [Engine API](#engine-api)
- [Malachite as a library](#malachite-as-a-library)
- [Connecting Malachite to Engine API](#connecting-malachite-to-engine-api)
- [Performance evaluation](#performance-evaluation)
- [Links](#links)
- [Running a local testnet](#running-a-local-testnet)

## Introduction

Ethereum's architecture consists of two primary layers: Consensus Layer (CL) and Execution Layer
(EL), with Engine API serving as the bridge between both. Malaketh-layered is a proof of concept
(PoC) to explore how Malachite can function as the consensus engine (CL) for Ethereum execution
clients (EL) through Engine API. Our goal is to show how Malachite can act as the consensus engine
for Layer 1 blockchains with Ethereum Virtual Machine (EVM) smart contracts, as well as a sequencer
for Layer 2 chains.

By leveraging Malachite's channel-based interface, we built a lightweight shim layer on top that
integrates seamlessly with any execution client supporting Engine API. For this PoC we have chosen
Reth as the execution client, but the design is agnostic and should work with any Engine
API-compliant client, such as Geth or Nethermind.

![malaketh-layered-0.png](docs/assets/malaketh-layered-0.png)

It's worth noting that Malaketh-layered is not an Ethereum consensus client. Ethereum's consensus
mechanism is based on Gasper, a hybrid of Casper FFG for finality and LMD-GHOST for fork choice,
where blocks are confirmed as immutable after two epochs, approximately 12.8 minutes. In constrast,
Malachite implements Tendermint, a BFT protocol with instant or single-slot finality. This means
Malaketh-layered is bringing instant finality to Ethereum execution but it cannot be used as a
direct replacement for Ethereum's consensus clients such as Lighthouse or Prysm.

## Engine API

Engine API plays a central role in Ethereum's post-merge architecture, defining a standardised RPC
interface between the Consensus Layer (CL) and Execution Layer (EL). The CL is responsible for
agreeing on the canonical chain and finalising blocks, while the EL handles block creation,
processing and execution, state management, blockchain storage, mempool management, RPC interfaces,
and more.

From the perspective of Engine API, the CL is a client that makes RPC calls with Engine API methods
to the EL, the RPC server. Key methods are:

- `forkchoiceUpdated`: Updates the execution client with the latest chain head and final block. If
  called with a `PayloadAttributes` parameter, it instructs the client to build a new block. This
  method also plays a role in Ethereum's finality mechanism by marking blocks as finalised.
- `getPayload`: Retrieves a newly constructed block from the execution client after calling
  `forkchoiceUpdated` with `PayloadAttributes`.
- `newPayload`: Submits a proposed block to the execution client for validation and inclusion in the
  chain. Note that it does not change the tip of the chain, which is the job of `forkchoiceUpdated`.

## Malachite as a library

Malachite offers three interfaces at different abstraction levels: Low-level, Actors, and Channels.
These interfaces range from fine-grained control to ready-to-use functionality.

In this PoC, we use the Channel-based interface, which prioritises ease of use over customisation.
It provides built-in synchronisation, crash recovery, networking for consensus voting, and block
propagation protocols. Application developers only need to interact with Malachite through a channel
that emits events, such as:

- `AppMsg::ConsensusReady { reply }`: Signals that Malachite is initialised and ready to begin
  consensus.
- `AppMsg::GetValue { height, round, timeout, reply }`: Requests a value (e.g., a block) from the
  application when the node is the proposer for a given height and round.
- `AppMsg::ReceivedProposalPart { from, part, reply }`: Delivers parts of a proposed value from
  other nodes, which are reassembled into a complete block.
- `AppMsg::Decided { certificate, reply }`: Notifies the application that consensus has been
  reached, providing a certificate with the decided value and supporting votes.

Malachite sends additional messages (e.g., for synchronisation), but we focus only on the core
events relevant to this integration. Each event includes a `reply` callback, allowing the
application to respond to Malachite.

For more details on Malachite's architecture and its three interfaces, check out the blog post [_The
Most Flexible Consensus API in the World_][flexible]. For a hands-on explanation of the Channels
API, see the [Malachite Channels tutorial][channels].

## Connecting Malachite to Engine API

Malaketh-layered is an application built on top of Malachite, which is unaware of Engine API and
only exposes the Channels interface.

The application includes two main components for interacting with the execution client:

- An RPC client with JWT authentication to send Engine API requests to the execution client.
- An internal state to keep track of values such as the latest block and the current height, round,
  and proposer. It also maintains persistent storage for proposals and block data to support block
  propagation.

Our integration revolves around three scenarios: initialising consensus, proposing a block as the
proposer, and voting as a non-proposer. Below we outline how Malachite's events map to Engine API
calls.

### Consensus initialisation

When Malachite starts, it sends a `AppMsg::ConsensusReady` event to signal the app that is ready.
For simplicity, we assume all nodes begin from a clean state (height one) without needing to sync
with an existing network. Each execution client initialises from the same genesis file, producing an
initial block (block number 1) with a `parent_hash` of `0x0`.

<img src="docs/assets/malaketh-layered-1.png" width="800" />

Malaketh-layered queries the execution client via the `eth_getBlockByNumber` RPC endpoint to fetch
the latest committed block (in this case, the genesis block). This block is stored in the
application state and serves as the base for building subsequent blocks.

### Proposing and committing a block

When a node becomes the proposer for a given height and round, the application receives from
Malachite a `AppMsg::GetValue`event. The node must propose a new block to the network. Here's how
the application drives this process:

1. The application calls `forkchoiceUpdated` with `PayloadAttributes` to instruct the execution
   client to build a new block. If the parameters are valid and everything goes as expected, the RPC
   method will return a `payload_id`.
2. Immediately, it calls `getPayload` with the `payload_id` of the previous step to retrieve an
   execution payload (the block).
3. The block is stored in the app’s local state and is sent back to Malachite via the `reply`
   callback, where it's propagated to other validators.

At this moment validators exchange Tendermint votes to reach consensus. Once agreed, Malachite emits
`AppMsg::Decided` to the application, which finalises the block in the execution client with the
following steps:

1. Retrieve the stored block and compute its hash.
2. Call `forkchoiceUpdated` with the block’s hash (no `PayloadAttributes`) to set the block as the
   head of the chain and finalise it.
3. Update the local state with the new block and certificate. Finally, signal Malachite to proceed
   to the next height.

<img src="docs/assets/malaketh-layered-2.png" width="800" />

### Voting and committing as a non-proposer

As a non-proposer, the application receives `AppMsg::ReceivedProposalPart` events with block
fragments. Once all parts are re-assembled, the block is stored locally. Eventually, Malachite
concludes consensus by emitting a `AppMsg::Decided` event. The application then calls `newPayload`
to submit the decided block to the execution client, followed by`forkchoiceUpdated` to update the
chain head and finalise the block.

<img src="docs/assets/malaketh-layered-3.png" width="800" />

## Performance evaluation

We deployed three nodes on a local network, each pairing a Malaketh-layered application with a Reth
instance. A separate application generates EIP-1559 token-transfer transactions (approximately 120
bytes each) at a rate of 1000 transactions per second (tps), sending them to one of the node’s
mempools for dissemination.

The network processes blocks at an average rate of 6 blocks per second, successfully handling all
transaction load. However, a significant number of these blocks are empty, even in the presence of
pending transactions in the mempool. This suggests that Reth does not take all available pending
transactions when constructing blocks. In the current setting, increasing the transaction load
beyond 1000 tps results in mempools getting full. We still need to investigate the exact cause,
which could be related to misconfiguration or Reth’s logic for block creation. In any case, we
believe that the system can handle much higher throughput once this issue is solved.

Check out the following section for reproducing these tests.

## Running a local testnet

### Requirements

The following tools are required to run a local testnet:

- **Docker** & **Docker Compose** - For running Reth execution clients and monitoring stack
- **Rust** (stable toolchain) - For building consensus and application binaries
  - Install: https://rustup.rs/
- **Foundry** - Ethereum development toolkit providing `forge` and `cast`
  - `forge` - For compiling Solidity contracts
  - `cast` - For RPC calls and blockchain queries
  - Install: https://book.getfoundry.sh/getting-started/installation
- **make** - Build automation tool
- **jq** - Command-line JSON processor (used in peer discovery scripts)
  - Install: `apt install jq` (Ubuntu/Debian) or `brew install jq` (macOS)
- **bash** (v4+) - For running setup scripts

Standard Unix tools (`xargs`, `ls`, `seq`, etc.) are also required but typically pre-installed on most systems.

### Setup and run

For local testnet, follow these steps:

1. Set the amount of validators to run:
   ```bash
   export NODE_COUNT=3
   ```
2. Build the Docker images and setup Forge:
   ```bash
   make build
   ```
3. Start the local testnet:
   ```bash
   make start
   ```

Node IDs are indexed 0-7 (example NODE_ID=0, NODE_ID=3 etc). To stop a single node, run:

```
make stop-node NODE_ID=<node_id>
```

To start a single node, run:

```
make start-node NODE_ID=<node_id>
```

To stop the entire testnet, run:

```
make stop-all
```

When finished, to stop and remove all containers, networks, and volumes created for the testnet, run:

```
make clean
```

### Inject transaction load

In a separate terminal, run the following command to send transactions during 60 seconds at a rate
of 1000 tx/s to one of Reth RPC endpoints.

```
cargo run --bin malachitebft-eth-utils spam --time=60 --rate=1000
```

> [!TIP]
> With the `cast` tool one can explore the blockchain by querying the execution client. For example:
>
> ```
> cast block-number                      # show the number of the latest finalised block
> cast block 3                           # show the block #3's content
> cast balances 0x...                    # show the balance of an account
> cast rpc txpool_status                 # show number of pending and queued transactions
> cast rpc eth_getTransactionCount 0x... # get latest nonce value used for given account
> ```

## Links

- Malachite architecture [https://github.com/circlefin/malachite/blob/13bca14cd209d985c3adf101a02924acde8723a5/ARCHITECTURE.md](https://github.com/circlefin/malachite/blob/13bca14cd209d985c3adf101a02924acde8723a5/ARCHITECTURE.md)
- Tutorial on Malachite’s Channel-based interface [https://github.com/circlefin/malachite/blob/13bca14cd209d985c3adf101a02924acde8723a5/docs/tutorials/channels.md][channels]
- The most flexible Consensus API in the world [https://informal.systems/blog/the-most-flexible-consensus-api-in-the-world][flexible]
- Engine API [https://github.com/ethereum/execution-apis/tree/main/src/engine](https://github.com/ethereum/execution-apis/tree/main/src/engine)
- Reth [https://github.com/paradigmxyz/reth](https://github.com/paradigmxyz/reth)

[malachite]: https://github.com/informalsystems/malachite
[engine-api]: https://github.com/ethereum/execution-apis/tree/main/src/engine
[foundry]: https://book.getfoundry.sh/getting-started/installation
[cast]: https://book.getfoundry.sh/cast/
[channels]: https://github.com/informalsystems/malachite/blob/13bca14cd209d985c3adf101a02924acde8723a5/docs/tutorials/channels.md
[flexible]: https://informal.systems/blog/the-most-flexible-consensus-api-in-the-world
