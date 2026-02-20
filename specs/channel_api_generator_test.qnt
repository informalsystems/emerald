// -*- mode: Bluespec; -*-

// Test module for channel_api_generator: instantiates with concrete nodes
// and checks the contract invariant via simulation.

module channel_api_generator_test {
  import channel_api_generator(NODES = List("node1", "node2", "node3")).* from "channel_api_generator"
}
