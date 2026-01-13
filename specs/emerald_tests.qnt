// -*- mode: Bluespec; -*-

// Quint runs for specific scenarios of emerald.

module emerald_tests {
  import basicSpells.* from "spells/basicSpells"
  import rareSpells.* from "spells/rareSpells"
  import emerald.* from "emerald"

  pure val firstProposal = {
    proposer: "node1",
    height: 1,
    round: 0,
    payload: 0
  }

  // All nodes start from from genesis
  run genesisStart =
    init
      // consensus_ready 
      .then("node1".with_cue(listen_consensus_ready, ()).perform(handle_consensus_ready))
      .then("node2".with_cue(listen_consensus_ready, ()).perform(handle_consensus_ready))
      .then("node3".with_cue(listen_consensus_ready, ()).perform(handle_consensus_ready))
      .expect(
        NODES.toSet().forall(node =>
           val st = s.system.get(node)
           and {
             st.phase == Ready,
             st.current_height == 1,
             st.current_round == 0,
             st.last_decided_height == 0,
             st.last_decided_payload == None,
             st.proposals.size() == 0
           }
         )
      )
      // started_round
      .then("node1".with_cue(listen_started_round, (1, 0)).perform(handle_started_round))
      .then("node2".with_cue(listen_started_round, (1, 0)).perform(handle_started_round))
      .then("node3".with_cue(listen_started_round, (1, 0)).perform(handle_started_round))
      .expect(
        and {
          "node1" == proposer_for(1, 0),
          NODES.toSet().forall(node =>
            s.system.get(node).phase == Working
          ),
        }
      )

  // All nodes decide on the same proposal
  run emeraldSingleHeightConsensusTest =
    genesisStart
      // get_value
      .then("node1".with_cue(listen_get_value, ()).perform(handle_get_value))
      .expect(
        s.system.get("node1").proposals == Set(firstProposal)
      )
      // received_proposal
      .then("node2".with_cue(listen_received_proposal, firstProposal).perform(handle_received_proposal))
      .then("node3".with_cue(listen_received_proposal, firstProposal).perform(handle_received_proposal))
      .expect(
        NODES.toSet().forall(node =>
          s.system.get(node).proposals == Set(firstProposal)
        )
      )
      // decided
      .then("node1".with_cue(listen_decided, firstProposal).perform(handle_decided))
      .then("node2".with_cue(listen_decided, firstProposal).perform(handle_decided))
      .then("node3".with_cue(listen_decided, firstProposal).perform(handle_decided))
      .expect(
        NODES.toSet().forall(node =>
          val st = s.system.get(node)
          and {
            st.last_decided_height == 1,
            st.last_decided_payload == Some(0),
            st.current_height == 2,
            st.current_round == 0
          }
        )
      )

  // Getting a value from proposer multiple times yields the same proposal
  run emeraldGetValueIdempotentTest =
    genesisStart
      // get_value (1st)
      .then("node1".with_cue(listen_get_value, ()).perform(handle_get_value))
      .expect(s.system.get("node1").proposals == Set(firstProposal))
      // get_value (2nd)
      .then("node1".with_cue(listen_get_value, ()).perform(handle_get_value))
      .expect(s.system.get("node1").proposals == Set(firstProposal)) // same proposal

  run emeraldNodeCrashAfterConsensusTest =
    genesisStart
      // first consensus
      .then("node1".with_cue(listen_get_value, ()).perform(handle_get_value))
      .then("node2".with_cue(listen_received_proposal, firstProposal).perform(handle_received_proposal))
      .then("node3".with_cue(listen_received_proposal, firstProposal).perform(handle_received_proposal))
      .then("node1".with_cue(listen_decided, firstProposal).perform(handle_decided))
      .then("node2".with_cue(listen_decided, firstProposal).perform(handle_decided))
      .then("node3".with_cue(listen_decided, firstProposal).perform(handle_decided))
      // node crashes
      .then("node1".step_with(node_crash))
      .expect(
        val st = s.system.get("node1")
        and {
          st.phase == Uninitialized,
          st.current_height == 0,
          st.current_round == 0,
          st.proposals.size() == 0,
          st.last_decided_height == 0,
          st.last_decided_payload == None
        }
      )
      // node restarts but loses its data
      .then("node1".with_cue(listen_consensus_ready, ()).perform(handle_consensus_ready))
      .expect(
        val st = s.system.get("node1")
        and {
          st.phase == Ready,
          st.current_height == 1,
          st.current_round == 0,
          st.proposals == Set()
        }
      )
      // start next height
      .then("node1".with_cue(listen_started_round, (1, 0)).perform(handle_started_round))
      .then("node2".with_cue(listen_started_round, (2, 0)).perform(handle_started_round))
      .then("node3".with_cue(listen_started_round, (2, 0)).perform(handle_started_round))
      .expect(
        and {
          NODES.toSet().forall(node =>
            s.system.get(node).phase == Working
          ),
          s.system.get("node1").current_height == 1, // <- node1 came back at hight 1
          s.system.get("node2").current_height == 2,
          s.system.get("node3").current_height == 2
        }
      )
      // get_value
      .then("node1".with_cue(listen_get_value, ()).perform(handle_get_value))
      .expect(
        s.system.get("node1").proposals == Set({
          ...firstProposal,
          payload: 1
        })
      )
      // TODO: Model recovery via value sync
}
