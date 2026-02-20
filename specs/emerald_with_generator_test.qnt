// -*- mode: Bluespec; -*-

// Test module for emerald_with_generator: instantiates with concrete nodes
// and checks invariants via simulation.

module emerald_with_generator_test {
  import emerald_with_generator(NODES = List("node1", "node2", "node3")).* from "emerald_with_generator"
}
