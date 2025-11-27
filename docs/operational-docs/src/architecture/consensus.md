# Consensus Engine

Emerald leverages [Malachite](https://github.com/circlefin/malachite) as its consensus engine. 
Malachite is the most optimized and lightweight evolution of the [Tendermint](https://arxiv.org/abs/1807.04938) Byzantine Fault Tolerant (BFT) protocol, 
which is the most battle-tested consensus protocol in blockchain today. 

## Key Features 

- **Separation from execution:** Consensus is separated from execution, allowing modular development and easy component customization.
- **Single-slot finality:** Transactions are finalized immediately once blocks are committed, without the risk of reorgs.
- **Low latency:** Malachite finalizes blocks in under one second, delivering the low-latency confirmation times required for high-performance institutional applications.
- **High throughput:** 
 > TODO: add throughput results 
- **Formally specified:** Malachite was formally specified and model checked using the [Quint specification language](https://quint-lang.org). 

## Emerald Integration 

Emerald uses Malachite's [channel-based interface](https://github.com/circlefin/malachite/blob/main/ARCHITECTURE.md#channel-based-interface) for integration.
This provides built-in synchronization, crash recovery, networking for consensus voting, and block propagation protocols.

Emerald, as a Malachite application, only needs to interact with the consensus engine through a channel that emits events:

- **`AppMsg::ConsensusReady { reply }`**: Signals that Malachite is initialized and ready to begin consensus.

- **`AppMsg::GetValue { height, round, timeout, reply }`**: Requests a value (e.g., a block) from the application when the node is the proposer for a given height and round.

- **`AppMsg::ReceivedProposalPart { from, part, reply }`**: Delivers parts of a proposed value from other nodes, which are reassembled into a complete block.

- **`AppMsg::Decided { certificate, reply }`**: Notifies the application that consensus has been reached, providing a certificate with the decided value and supporting votes.

> TODO
> - what other things we added here (crash recovery, block sync ... )
> - do we want to add a section about Tendermint and how it works? 
> - do we want to mention 5f+1 or not yet? 