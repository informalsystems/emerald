// -*- mode: Bluespec; -*-

// Test module for channel_api_generator: instantiates with concrete nodes
// and checks the contract invariant via simulation.
//
// 4 nodes: with equal voting power and the >2/3 quorum threshold, 3 out of 4
// nodes must support a proposal before any node can decide. This allows
// traces where one node crashes and the remaining 3 still make progress.

module channel_api_generator_test {
  import channel_api_contract.* from "channel_api_contract"

  pure val NODES = List("node1", "node2", "node3", "node4")

  pure def test_proposer_for(height: Height, round: Round): Node =
    NODES[(height - 1 + round) % NODES.length()]

  import channel_api_generator(
    NODES = NODES,
    proposer_for = test_proposer_for,
  ) as gen from "channel_api_generator"

  // Re-export generator state for simulation
  action init = gen::init
  action step = gen::step
  val contractInv = gen::contractInv
  val applicationSafetyInv = gen::applicationSafetyInv
}
