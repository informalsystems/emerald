// -*- mode: Bluespec; -*-

// Test module for emerald_with_generator: instantiates with concrete nodes
// and checks invariants via simulation.
//
// 4 nodes: allows progress with one crashed node (3/4 > 2/3 quorum).

module emerald_with_generator_test {
  import rareSpells.* from "spells/rareSpells"
  import emerald_with_generator(NODES = List("node1", "node2", "node3", "node4")).* from "emerald_with_generator"

  // Liveness: every node eventually responds to ConsensusReady and starts
  // consensus. Checked as bounded liveness over finite traces — semi-formal.
  // Requires fair scheduling (the simulator's oneOf provides probabilistic
  // fairness). Full liveness proof would require export to TLA+.
  temporal allNodesStartConsensus = NODES.toSet().forall(node =>
    eventually(emeraldState.get(node).mem.phase != Uninitialized)
  )
}
