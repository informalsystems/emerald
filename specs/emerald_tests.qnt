module emerald_test {
  import basicSpells.* from "spells/basicSpells"
  import rareSpells.* from "spells/rareSpells"
  import emerald_app.* from "emerald_app"

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
             st.latest_block_height == 0,
             st.latest_block_payload == None,
             st.proposals.size() == 0
           }
         )
      )
      // started_round
      .then("node1".with_cue(listen_started_round, ()).perform(handle_started_round))
      .then("node2".with_cue(listen_started_round, ()).perform(handle_started_round))
      .then("node3".with_cue(listen_started_round, ()).perform(handle_started_round))
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
            st.latest_block_height == 1,
            st.latest_block_payload == Some(0),
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
}
