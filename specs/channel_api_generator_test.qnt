// -*- mode: Bluespec; -*-

// Test module for channel_api_generator: instantiates with concrete nodes
// and checks the contract invariant via simulation.

module channel_api_generator_test {
  import channel_api_contract.* from "channel_api_contract"

  pure val NODES = List("node1", "node2", "node3")

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
}
