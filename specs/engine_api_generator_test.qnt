// -*- mode: Bluespec; -*-

// Test module for engine_api_generator: instantiates with concrete nodes
// and checks the contract invariant via simulation.
//
// 4 nodes: consistent with channel API tests (allows fault tolerance
// exploration with one crashed node).

module engine_api_generator_test {
  import engine_api_contract.* from "engine_api_contract"

  pure val NODES = List("node1", "node2", "node3", "node4")

  import engine_api_generator(
    NODES = NODES,
  ) as gen from "engine_api_generator"

  // Re-export generator state for simulation
  action init = gen::init
  action step = gen::step
  val contractInv = gen::contractInv
}
