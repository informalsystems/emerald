// -*- mode: Bluespec; -*-

// =============================================================================
// Emerald Specification Tests
// =============================================================================
//
// This module contains deterministic test scenarios for the Emerald
// specification, validating specific execution paths through the protocol's
// state machine.
//
// Note that tests are prefixed with "emerald" in order to isolate them from
// tests in other imported Quint modules. Run Emerald-only tests with:
//
// ```
// quint test emerald_tests.qnt --match "emerald"
// ```

module emerald_tests {
  import basicSpells.* from "spells/basicSpells"
  import rareSpells.* from "spells/rareSpells"
  import emerald(NODES = List("node1", "node2", "node3")).* from "emerald"

  // =============================================================================
  // MALACHITE INPUTS
  // =============================================================================

  run consensusReady(node: Node, height: Height, round: Round): bool =
    node
      .with_cue(listen_consensus_ready, ())
      .perform(handle_consensus_ready)
      .expect(
        val st = s.system.get(node)
        and {
          st.phase == Ready,
          st.current_height == height,
          st.current_round == round
         }
      )

  run startedRound(node: Node, height: Height, round: Round): bool =
    node
      .with_cue(listen_started_round, (height, round))
      .perform(handle_started_round)
      .expect(s.system.get(node).phase == Working)

  run getValue(node: Node, proposal: Proposal): bool =
    node
      .with_cue(listen_get_value, ())
      .perform(handle_get_value)
      .expect(proposal.in(s.system.get(node).proposals))

  run receivedProposal(node: Node, proposal: Proposal): bool =
    node
      .with_cue(listen_received_proposal, proposal)
      .perform(handle_received_proposal)
      .expect(proposal.in(s.system.get(node).proposals))

  run decided(node: Node, proposal: Proposal): bool =
    node
      .with_cue(listen_decided, proposal)
      .perform(handle_decided)
      .expect(
        val st = s.system.get(node)
        and {
          st.last_decided_height == proposal.height,
          st.last_decided_payload == Some(proposal.payload),
          st.current_height == proposal.height + 1,
          st.current_round == 0,
          st.phase == Ready
        }
      )

  run processSyncedValue(node: Node, proposal: Proposal): bool =
    node
      .with_cue(listen_process_synced_value, proposal)
      .perform(handle_process_synced_value)
      .expect(proposal.in(s.system.get(node).proposals))

  // =============================================================================
  // FAILURES
  // =============================================================================

  run timeout(node: Node, height: Height, round: Round): bool =
    node
      .with_cue(listen_timeout, ())
      .perform(consensus_timeout)
      .expect(
        val timeout = Timeout({ height: height, round: round })
        timeout.in(s.events.get(node))
      )

  run crash(node: Node): bool =
    node
      .step_with(node_crash)
      .expect(s.system.get(node) == initialize(node))

  run restart(node: Node): bool =
    node
      .step_with(process_restart)
      .expect(
        val st = s.system.get(node)
        and {
          st.phase == Uninitialized,
          st.current_height == 0,
          st.current_round == 0
        }
      )

  // =============================================================================
  // HELPER RUNS AND FUNCTIONS
  // =============================================================================

  pure def proposal(height: Height, round: Round, payload: Payload): Proposal =
    {
      height: height,
      round: round,
      payload: payload,
      proposer: proposer_for(height, round)
    }

  // All nodes start from genesis
  run genesisStart =
    init
      .then("node1".consensusReady(1, 0))
      .then("node2".consensusReady(1, 0))
      .then("node3".consensusReady(1, 0))
      .expect(
        NODES.toSet().forall(node =>
           val st = s.system.get(node)
           and {
             st.last_decided_height == 0,
             st.last_decided_payload == None,
             st.proposals.size() == 0
           }
         )
      )
      .then("node1".startedRound(1, 0))
      .then("node2".startedRound(1, 0))
      .then("node3".startedRound(1, 0))
      .expect(proposer_for(1, 0) == "node1")

  // Nodes 1 and 2 decided on heights 1 and 2 while node 3 stalled
  run node3Stalled =
    genesisStart
      .then("node1".getValue(proposal(1, 0, 0)))
      .then("node2".receivedProposal(proposal(1, 0, 0)))
      .then("node1".decided(proposal(1, 0, 0)))
      .then("node2".decided(proposal(1, 0, 0)))
      .then("node1".startedRound(2, 0))
      .then("node2".startedRound(2, 0))
      .expect(proposer_for(2, 0) == "node2")
      .then("node2".getValue(proposal(2, 0, 1)))
      .then("node1".receivedProposal(proposal(2, 0, 1)))
      .then("node2".decided(proposal(2, 0, 1)))
      .then("node1".decided(proposal(2, 0, 1)))

  // =============================================================================
  // TEST SCENARIOS
  // =============================================================================

  // Happy path: all nodes decided on a single height
  run emeraldSingleHeightConsensusTest =
    genesisStart
      .then("node1".getValue(proposal(1, 0, 0)))
      .then("node2".receivedProposal(proposal(1, 0, 0)))
      .then("node3".receivedProposal(proposal(1, 0, 0)))
      .then("node1".decided(proposal(1, 0, 0)))
      .then("node2".decided(proposal(1, 0, 0)))
      .then("node3".decided(proposal(1, 0, 0)))
      .expect(NODES.toSet().forall(node =>
        val st = s.system.get(node)
        and {
          st.last_decided_height == 1,
          st.last_decided_payload == Some(0)
        }
      ))

  // Unhappy path: nodes decide after a consensus timeout
  run emeraldTimeoutAndDecideTest =
    genesisStart
      .then("node1".getValue(proposal(1, 0, 0)))
      .then("node1".timeout(1, 0))
      .then("node2".timeout(1, 0))
      .then("node3".timeout(1, 0))
      .then("node1".startedRound(1, 1))
      .then("node2".startedRound(1, 1))
      .then("node3".startedRound(1, 1))
      .expect(proposer_for(1, 1) == "node2")
      .then("node2".getValue(proposal(1, 1, 1)))
      .then("node1".receivedProposal(proposal(1, 1, 1)))
      .then("node3".receivedProposal(proposal(1, 1, 1)))
      .then("node2".decided(proposal(1, 1, 1)))
      .then("node1".decided(proposal(1, 1, 1)))
      .then("node3".decided(proposal(1, 1, 1)))
      .expect(NODES.toSet().forall(node =>
        val st = s.system.get(node)
        and {
          st.last_decided_height == 1,
          st.last_decided_payload == Some(1)
        }
      ))

  // Unhappy path: stalled node catches up on consensus
  run emeraldStalledNodeCatchesUpViaSyncTest =
    node3Stalled
      .then("node3".processSyncedValue(proposal(1, 0, 0)))
      .then("node3".processSyncedValue(proposal(2, 0, 1)))
      .then("node3".decided(proposal(1, 0, 0)))
      .then("node3".startedRound(2, 0))
      .then("node3".decided(proposal(2, 0, 1)))
      .expect(NODES.toSet().forall(node =>
        val st = s.system.get(node)
        and {
          st.last_decided_height == 2,
          st.last_decided_payload == Some(1)
        }
      ))

  // Unhappy path: stalled node catches up after crash
  run emeraldStalledNodeCatchesUpAfterCrashTest =
    node3Stalled
      .then("node3".crash())
      .then("node3".consensusReady(1, 0))
      .then("node3".startedRound(1, 0))
      .then("node3".processSyncedValue(proposal(1, 0, 0)))
      .then("node3".processSyncedValue(proposal(2, 0, 1)))
      .then("node3".decided(proposal(1, 0, 0)))
      .then("node3".startedRound(2, 0))
      .then("node3".decided(proposal(2, 0, 1)))
      .expect(NODES.toSet().forall(node =>
        val st = s.system.get(node)
        and {
          st.last_decided_height == 2,
          st.last_decided_payload == Some(1)
        }
      ))

  // Unhappy path: stalled node catches up after a restart
  run emeraldStalledNodeCatchesUpAfterRestartTest =
    node3Stalled
      .then("node3".processSyncedValue(proposal(1, 0, 0)))
      .then("node3".decided(proposal(1, 0, 0)))
      .then("node3".restart())
      .then("node3".consensusReady(2, 0))
      .then("node3".startedRound(2, 0))
      .then("node3".processSyncedValue(proposal(2, 0, 1)))
      .then("node3".decided(proposal(2, 0, 1)))
      .expect(NODES.toSet().forall(node =>
        val st = s.system.get(node)
        and {
          st.last_decided_height == 2,
          st.last_decided_payload == Some(1)
        }
      ))
}
