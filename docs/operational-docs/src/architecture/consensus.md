# Consensus Layer

Emerald leverages [Malachite](https://github.com/circlefin/malachite) as its consensus engine. 
Malachite is the most optimized and lightweight evolution of the [Tendermint](https://arxiv.org/abs/1807.04938) Byzantine Fault Tolerant (BFT) protocol, 
which is the most battle-tested consensus protocol in blockchain today. 

## Key Properties 

**Separation From Execution.** Consensus is separated from execution, allowing modular development and easy component customization.

**Single-Slot Finality.** Transactions are finalized immediately once blocks are committed, without the risk of reorgs.

**Low Latency.** Malachite finalizes blocks in under one second, delivering the low-latency confirmation times required for high-performance institutional applications.

**High Throughput.** 
 > TODO: add throughput results 

**Formally Specified.** Malachite was formally specified and model checked using the [Quint specification language](https://quint-lang.org). 

## Malachite Integration 

Emerald uses Malachite's [channel-based interface](https://github.com/circlefin/malachite/blob/main/ARCHITECTURE.md#channel-based-interface) for integration.
This provides built-in synchronization, crash recovery, networking for consensus voting, and block propagation protocols.

Emerald, as a Malachite application, only needs to interact with the consensus engine through a channel that emits events:

- `AppMsg::ConsensusReady { reply }`: Signals that Malachite is initialized and ready to begin consensus.

- `AppMsg::GetValue { height, round, timeout, reply }`: Requests a value (e.g., a block) from the application when the node is the proposer for a given height and round.

- `AppMsg::ReceivedProposalPart { from, part, reply }`: Delivers parts of a proposed value from other nodes, which are reassembled into a complete block.

- `AppMsg::Decided { certificate, reply }`: Notifies the application that consensus has been reached, providing a certificate with the decided value and supporting votes.

**Note.** Emerald doesn't currently support the following Malachite events: `AppMsg::RestreamProposal` , `AppMsg::ExtendVote`, `AppMsg::VerifyVoteExtension`.

## Additional Features

### Value Sync 

Malachite expects nodes that fall behind to use a different protocol to catch up without participating in consensus. 
In Malachite terminology, this protocol is referred to as _Value Sync_ (as the nodes sync on the past values decided by Malachite). 
In the context of Emerald, these values consists of Ethereum blocks. 

Emerald handles the following events emitted by Malachite, in order to implement the Value Sync protocol:

- `AppMsg::ProcessSyncedValue { height, round, proposer, value_bytes, reply }`: 
  Used to process and validate values received from other peers while syncing. 
  The values are validated against the execution client and stored for processing in the Decided function.

- `AppMsg::GetDecidedValue { height, reply }`: 
  Used to provide peers that are behind with already decided values stored. 
  Note that Emerald caches a certain number of blocks locally, but the actual block history is stored in the execution client.

- `AppMsg::GetHistoryMinHeight`: Used to update peers on the minimum height for which the local node has a block.