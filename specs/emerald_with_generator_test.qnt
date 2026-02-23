// -*- mode: Bluespec; -*-

// Test module for emerald_with_generator: instantiates with concrete nodes
// and checks invariants via simulation.
//
// 4 nodes: allows progress with one crashed node (3/4 > 2/3 quorum).

module emerald_with_generator_test {
  import emerald_with_generator(NODES = List("node1", "node2", "node3", "node4")).* from "emerald_with_generator"
}
