// -*- mode: Bluespec; -*-

// =============================================================================
// Emerald Specification
// =============================================================================
//
// This specification models the Emerald application layer, which sits on top of
// the Malachite consensus engine and manages the lifecycle of blockchain blocks
// across multiple distributed nodes.
//
// We model Malachite consensus inputs and how Emerald reacts to them. The
// network, storage, and blockchain systems are considered implementation
// details and are not modeled here.
//
// For model-based testing (MBT), actions and relevant data are tracked as part
// of global state so they can be made available to Quint Connect for testing.

module emerald {
  import basicSpells.* from "spells/basicSpells"
  import rareSpells.* from "spells/rareSpells"
  import emerald_types.* from "emerald_types"
  import choreo(processes = NODES.toSet()) as choreo from "choreo"

  // =============================================================================
  // PROTOCOL CONFIGURATION
  // =============================================================================

  // The Emerald nodes to test. E.g.: List("node1", "node2", "node3")
  const NODES: List[Node]

  // =============================================================================
  // CHOREO BOILERPLATE
  // =============================================================================

  type LocalState = choreo::LocalState[Node, StateFields]
  type LocalContext = choreo::LocalContext[Node, StateFields, Message, Event, Extensions]
  type Transition = choreo::Transition[Node, StateFields, Message, Event, CustomEffects]
  type GlobalContext = choreo::GlobalContext[Node, StateFields, Message, Event, Extensions]

  // =============================================================================
  // HELPER FUNCTIONS
  // =============================================================================

  // Return the proposer for the given height and round
  pure def proposer_for(height: Height, round: Round): Node =
    NODES[(height - 1 + round) % NODES.length()]

  // Returns the next block payload.
  //
  // For simplicity, we generate payloads as sequential numbers based on how
  // many proposals there are, thus ensuring a unique payload per proposal
  // recorded.
  pure def next_payload(ctx: LocalContext): Payload =
    ctx.extensions.all_proposals.size()

  // Get a proposal from the local state for the given height and round, if any.
  pure def get_local_proposal(s: LocalState, height: Height, round: Round): Option[Proposal] =
    s.proposals.find(p => p.height == height and p.round == round)

  // Get a decided proposal on the global scope for the given height, if any.
  pure def get_global_decision(ctx: LocalContext, height: Height): Option[Proposal] =
    ctx.extensions.decided_proposals.find(p => p.height == height)

  // =============================================================================
  // TRANSITION FUNCTIONS
  // =============================================================================

  // ===== ConsensusReady: Initialize the system =====
  // 
  // Assumption: When starting the process, Malachite will call Emerald with the
  // ConsensusReady message once it's done initializing. In response, Emerald
  // will start at the last decided height + 1.

  pure def listen_consensus_ready(ctx: LocalContext): Set[()] =
    if (ctx.state.phase == Uninitialized) Set(()) else Set()

  pure def handle_consensus_ready(ctx: LocalContext, _params: ()): Transition = {
    val s = ctx.state
    val ext = ctx.extensions

    val last_decided_height = ext.last_decided_height.get(s.process_id)

    val s0 =
      match get_global_decision(ctx, last_decided_height) {
        // Genesis start: Emerald do not have any previously known decided
        // value. Consensus height begins in 0.
        | None => {
          ...s,
          consensus_height: 1,
          last_decided_height: 0,
          last_decided_payload: None
        }
        // Non-genesis start: Emerald knows a previously decided value.
        // Consensus height is the last decided height.
        | Some(last_decided) => {
          ...s,
          consensus_height: last_decided.height + 1,
          last_decided_height: last_decided.height,
          last_decided_payload: Some(last_decided.payload)
        }
      }

    {
      post_state: {
        ...s0,
        phase: Ready,
        consensus_round: 0
      },
      effects: Set(
        choreo::CustomEffect(RecordAction(
          ConsensusReadyAction({ node: s.process_id })
        ))
      )
    }
  }

  // ===== StartedRound: Enter new round =====
  //
  // Assumption: After ConsensusReady, on new heights, or upon a timeout,
  // Malachite will call StartedRound informing Emerald of a new round starting.
  // In response, Emerald will track the consensus height and round.

  pure def listen_started_round(ctx: LocalContext): Set[(Height, Round)] = {
    val s = ctx.state

    val timeout =
      ctx.events.find(e => match e {
        | Timeout(t) => and {
            t.height == s.consensus_height,
            t.round == s.consensus_round,
          }
      })

    if (timeout != None)       Set((s.consensus_height, s.consensus_round + 1))
    else if (s.phase == Ready) Set((s.consensus_height, s.consensus_round))
    else                       Set()
  }

  pure def handle_started_round(ctx: LocalContext, params: (Height, Round)): Transition = {
    val s = ctx.state
    val (height, round) = params

    val proposer = proposer_for(height, round)

    {
      post_state: {
        ...s,
        phase: Working,
        consensus_height: height,
        consensus_round: round,
      },
      effects: Set(
        choreo::CustomEffect(RecordAction(
          StartedRoundAction({
            node: s.process_id,
            height: height,
            round: round,
            proposer: proposer
          })
        ))
      ),
    }
  }

  // ===== GetValue: Build and propose new value =====
  //
  // Assumption: After starting a round, Malachite will call GetValue on the
  // Emerald node that is the proposer for the consensus height and round. In
  // response, Emerald will either propose a new value or return an already
  // proposed value.

  pure def listen_get_value(ctx: LocalContext): Set[()] = {
    val s = ctx.state
    val proposer = proposer_for(s.consensus_height, s.consensus_round)
    if (s.phase == Working and s.process_id == proposer) Set(()) else Set()
  }

  pure def handle_get_value(ctx: LocalContext, _params: ()): Transition = {
    val s = ctx.state

    val proposal =
      match get_local_proposal(s, s.consensus_height, s.consensus_round) {
        | Some(proposal) => proposal
        | None => {
            height: s.consensus_height,
            round: s.consensus_round,
            proposer: s.process_id,
            payload: next_payload(ctx)
          }
      }

    {
      post_state: {
        ...s,
        proposals: s.proposals.setAdd(proposal)
      },
      effects: Set(
        choreo::CustomEffect(RecordProposal(proposal)),
        choreo::CustomEffect(RecordAction(
          GetValueAction({
            node: s.process_id,
            height: s.consensus_height,
            round: s.consensus_round,
            proposal: proposal,
          })
        ))
      )
    }
  }

  // ===== ReceivedProposal: Receive a proposal =====
  //
  // Assumption: Malachite will call ReceivedProposal on the Emerald node,
  // informing it of a new proposal being received for a height >=
  // consensus_height. In response, Emerald will store the new proposal in its
  // local state.

  pure def listen_received_proposal(ctx: LocalContext): Set[Proposal] = {
    val s = ctx.state

    if (s.phase == Working)
      ctx.extensions.all_proposals.filterMap(p => {
        if (and {
          // Note that a node does not receive a proposal of its own. Recovering
          // its proposals after a crash is done via the sync protocol instead.
          p.proposer != s.process_id,
          p.height >= s.consensus_height,
          not(p.in(s.proposals))
        })
          Some(p)
        else
          None
      })
    else
      Set()
  }

  pure def handle_received_proposal(ctx: LocalContext, proposal: Proposal): Transition = {
    val s = ctx.state

    {
      post_state: {
        ...s,
        proposals: s.proposals.setAdd(proposal)
      },
      effects: Set(
        choreo::CustomEffect(RecordAction(
          ReceivedProposalAction({
            node: s.process_id,
            proposal: proposal
          })
        ))
      )
    }
  }

  // ===== Decided: Commit decided value =====
  //
  // Assumption: Eventually a node decides on the proposal. Malachite calls
  // Decided on an Emerald node to inform it of a decision. In response, Emerald
  // tracks the decision and moves to the next height.

  pure def listen_decided(ctx: LocalContext): Set[Proposal] = {
    val s = ctx.state

    if (s.phase == Working or s.phase == Syncing)
      match get_global_decision(ctx, s.consensus_height) {
        // There isn't a decision for the consensus height. Let's mimic
        // Malachite consensus and have some node decide on a proposal for the
        // consensus height and round.
        | None =>
            match get_local_proposal(s, s.consensus_height, s.consensus_round) {
              | Some(p) => Set(p)
              | None    => Set()
            }
        // A decision has been made for the consensus height. Let's replicate
        // the decision as long as the Emerald node is at the same height.
        | Some(p) =>
            if (and {
              p.in(s.proposals),
              p.height == s.consensus_height
            })
              Set(p)
            else
              Set()
      }
    else
      Set()
  }

  pure def handle_decided(ctx: LocalContext, decided: Proposal): Transition = {
    val s = ctx.state

    {
      post_state: {
        ...s,
        // Start next height by moving back to the ready phase, which enables
        // the handle_started_round transition.
        phase: Ready,
        consensus_height: decided.height + 1,
        consensus_round: 0,
        // Records the last decided block
        last_decided_height: decided.height,
        last_decided_payload: Some(decided.payload),
      },
      effects: Set(
        choreo::CustomEffect(RecordDecision((s.process_id, decided))),
        choreo::CustomEffect(RecordAction(
          DecidedAction({
            node: s.process_id,
            proposal: decided
          })
        ))
      )
    }
  }

  // ===== ProcessSyncedValue: Process synced block when catching up =====
  //
  // Assumption: Malachite may call ProcessSyncedValue on an Emerald node with
  // decided proposals for the consensus heigh. In response, Emerald tracks the
  // newly received proposal.
  //
  // Note that a later call to Decided is made by Malachite to confirm the
  // received decided proposals.

  pure def listen_process_synced_value(ctx: LocalContext): Set[Proposal] = {
    val s = ctx.state

    if (s.phase == Working or s.phase == Syncing)
      val decided_proposal_for_height =
        ctx.extensions.decided_proposals
          .find(p => and {
            p.height == s.consensus_height,
            not(p.in(s.proposals))
          })

      match decided_proposal_for_height {
        | Some(p) => Set(p)
        | None    => Set()
      }
    else
      Set()
  }

  pure def handle_process_synced_value(ctx: LocalContext, proposal: Proposal): Transition = {
    val s = ctx.state

    {
      post_state: {
        ...s,
        phase: Syncing,
        proposals: s.proposals.setAdd(proposal)
      },
      effects: Set(
        choreo::CustomEffect(RecordAction(
          ProcessSyncedValueAction({
            node: s.process_id,
            proposal: proposal
          })
        ))
      )
    }
  }

  // ===== GetDecidedValue: Provide decided value for sync =====
  //
  // Assumption: Malachite may call an Emerald node asking for a decision at a
  // given height. In response, Emerald returns the decided value, if any.

  pure def listen_get_decided_value(ctx: LocalContext): Set[Height] = {
    val s = ctx.state

    val max_decided_height =
      ctx.extensions
        .last_decided_height
        .values()
        .fold(0, max)

    1.to(max_decided_height)
  }

  pure def handle_get_decided_value(ctx: LocalContext, height: Height): Transition = {
    val s = ctx.state

    // Note that `listen_get_decided_value` MUST only feed decided heights here.
    val decided_proposal = get_global_decision(ctx, height).unwrap()

    // Only return the decided proposal if the node knows it has been decided.
    //
    // Note that Emerald does not return decided proposals for the current
    // consensus height.
    val known_decided =
      if (and {
        s.consensus_height > decided_proposal.height,
        s.last_decided_height >= decided_proposal.height
      })
        Some(decided_proposal)
      else
        None

    {
      post_state: s,
      effects: Set(
        choreo::CustomEffect(RecordAction(
          GetDecidedValueAction({
            node: s.process_id,
            height: height,
            proposal: known_decided
          })
        ))
      )
    }
  }

  // TODO: Implement missing Malachite inputs
  // + GetHistoryMinHeight
  // + RestreamProposal
  //
  // TODO: Model additional behavior
  // + State pruning

  // ===== NodeCrash: A node crashes and comes back without any state =====
  //
  // Assumption: A whole node can crash at any point and lose all its data. When
  // restarted, it catches up with others via ReceivedProposal or
  // ProcessSyncedValue.

  pure def node_crash(ctx: LocalContext): Set[Transition] = {
    val s = ctx.state

    Set({
      post_state: initialize(s.process_id),
      effects: Set(
        choreo::CustomEffect(RecordAction(
          Failure({
            node: s.process_id,
            height: s.consensus_height,
            mode: NodeCrash,
          })
        ))
      )
    })
  }

  // ===== NodeRestart: A node restarts but preserves its state =====
  //
  // Assumption: A node can restart without data loss. When restarted, it comes
  // back at height and round zero, then ConsensusReady and StartedRound will
  // move it back to the height it was before restarting.
  //
  // Note that from the perspective of model `node_restart` has the same
  // behavior of `process_restart`, however, MBT simulates different types of
  // failures in the test environment where the former restarts all node's
  // processes while the latter only restarts the Emerald process.

  pure def node_restart(ctx: LocalContext): Set[Transition] = {
    val s = ctx.state

    Set({
      post_state: {
        ...initialize(s.process_id),
        proposals: s.proposals
      },
      effects: Set(
        choreo::CustomEffect(RecordAction(
          Failure({
            node: s.process_id,
            height: s.consensus_height,
            mode: NodeRestart
          })
        ))
      )
    })
  }

  // ===== ProcessRestart: The Emerald process restarts but preserves its state =====
  //
  // Assumption: The Emerald process can restart without data loss. When
  // restarted, it comes back at height and round zero, then ConsensusReady and
  // StartedRound will move it back to the height it was before restarting.
  //
  // Note that from the perspective of model `process_restart` has the same
  // behavior of `node_restart`. See docs at `node_restart`.

  pure def process_restart(ctx: LocalContext): Set[Transition] = {
    val s = ctx.state

    Set({
      post_state: {
        ...initialize(s.process_id),
        proposals: s.proposals
      },
      effects: Set(
        choreo::CustomEffect(RecordAction(
          Failure({
            node: s.process_id,
            height: s.consensus_height,
            mode: ProcessRestart
          })
        ))
      )
    })
  }

  // ===== ConsensusTimeout: A process timed out on consensus =====
  //
  // Assumption: Malachite may timeout while waiting for consensus on an
  // undecided height. Then, it calls StartedRound on the Emerald node to inform
  // of a new round starting.

  pure def listen_timeout(ctx: LocalContext): Set[()] = {
    val s = ctx.state
    // Note that nodes that aren't working are not tracking timeouts.
    if (s.phase == Working) Set(()) else Set()
  }

  pure def consensus_timeout(ctx: LocalContext, _params: ()): Transition = {
    val s = ctx.state

    {
      post_state: s,
      effects: Set(
        choreo::TriggerEvent(Timeout({
          height: s.consensus_height,
          round: s.consensus_round
        })),
        choreo::CustomEffect(RecordAction(
          Failure({
            node: s.process_id,
            height: s.consensus_height,
            mode: ConsensusTimeout
          })
        ))
      )
    }
  }

  // =============================================================================
  // MAIN LISTENER
  // =============================================================================

  pure def main_listener(ctx: LocalContext): Set[Transition] = {
    Set(
      // Initialization
      choreo::cue(ctx, listen_consensus_ready, handle_consensus_ready),

      // Round progression
      choreo::cue(ctx, listen_started_round, handle_started_round),

      // Proposal building and broadcast
      choreo::cue(ctx, listen_get_value, handle_get_value),
      choreo::cue(ctx, listen_received_proposal, handle_received_proposal),

      // Consensus decision
      choreo::cue(ctx, listen_decided, handle_decided),

      // Synchronization protocol
      choreo::cue(ctx, listen_process_synced_value, handle_process_synced_value),
      choreo::cue(ctx, listen_get_decided_value, handle_get_decided_value),

      // Failures
      node_crash(ctx),
      node_restart(ctx),
      process_restart(ctx),
      choreo::cue(ctx, listen_timeout, consensus_timeout)
    ).flatten()
  }

  // =============================================================================
  // CUSTOM EFFECT PROCESSOR
  // =============================================================================

  pure def apply_custom_effect(ctx: GlobalContext, effect: CustomEffects): GlobalContext = {
    val ext = ctx.extensions

    match effect {
      | RecordProposal(proposal) => {
          ...ctx,
          extensions: {
            ...ext,
            all_proposals: ext.all_proposals.setAdd(proposal)
          }
        }
      | RecordDecision(args) =>
        val (node, proposal) = args
        {
          ...ctx,
          extensions: {
            ...ext,
            decided_proposals: ext.decided_proposals.setAdd(proposal),
            last_decided_height: ext.last_decided_height.put(node, proposal.height)
          }
        }
      | RecordAction(action_taken) =>
          val ext0 = {
            ...ctx.extensions,
            action_taken: action_taken
          }
          match action_taken {
            | Failure(args) =>
                val { node, height, mode } = args
                val s = ctx.system.get(node)
                {
                  ...ctx,
                  extensions: {
                    ...ext0,
                    failures: ext0.failures.setAdd(args),
                    // Note that during a node crash, the node loses track of its
                    // last decided height and restarts from genesis.
                    last_decided_height:
                      if (mode == NodeCrash) ext0.last_decided_height.put(node, 0)
                      else ext0.last_decided_height
                  },
                  // Note that after a crash/restart the node loses its timeouts.
                  events:
                    if (mode != ConsensusTimeout) ctx.events.put(node, Set())
                    else ctx.events
                }
            | _ => { ...ctx, extensions: ext0 }
          }
    }
  }

  // =============================================================================
  // INITIALIZATION
  // =============================================================================

  pure def initialize(node: Node): LocalState = 
    {
      process_id: node,
      phase: Uninitialized,
      consensus_height: 0,
      consensus_round: 0,
      proposals: Set(),
      last_decided_height: 0,
      last_decided_payload: None
    }

  action init =
    val nodes = NODES.toSet()
    choreo::init({
      system: nodes.mapBy(n => initialize(n)),
      messages: nodes.mapBy(n => Set()),
      events: nodes.mapBy(n => Set()),
      extensions: {
        action_taken: InitAction,
        all_proposals: Set(),
        decided_proposals: Set(),
        last_decided_height: nodes.mapBy(n => 0),
        failures: Set()
      }
    })

  // =============================================================================
  // STEP FUNCTIONS
  // =============================================================================

  action step = choreo::step(main_listener, apply_custom_effect)

  action step_with_filter(f: (LocalContext, Transition) => bool): bool =
    choreo::step(ctx => main_listener(ctx).filter(t => f(ctx, t)), apply_custom_effect)

  // =============================================================================
  // PROPERTIES AND INVARIANTS
  // =============================================================================

  // Safety: All nodes agree on decided blocks.
  val agreement =
    NODES.toSet().forall(n1 =>
      NODES.toSet().forall(n2 =>
        val st1 = choreo::convert_context(choreo::s, n1).state
        val st2 = choreo::convert_context(choreo::s, n2).state

        st1.last_decided_height == st2.last_decided_height implies
          st1.last_decided_payload == st2.last_decided_payload
      )
    )

  // Assumption: All nodes know decided proposals.
  val completion =
    NODES.toSet().forall(n =>
      val ctx = choreo::convert_context(choreo::s, n)
      val st = ctx.state

      1.to(st.last_decided_height).forall(height =>
        val decided = get_global_decision(ctx, height).unwrap()
        decided.in(st.proposals)
      )
    )

  // Assumption: After initialization, Emerald's consensus height IS ALWAYS
  // greater than the last decided height.
  val consensus_height_gt_last_decided_height =
    NODES.toSet().forall(n =>
      val st = choreo::convert_context(choreo::s, n).state

      st.consensus_height > 0 implies
        st.consensus_height > st.last_decided_height
    )

  // =============================================================================
  // WIRING CHOREO FOR TESTING
  // =============================================================================

  val s = choreo::s

  def with_cue(process, listen_fn, params) =
    choreo::with_cue(process, listen_fn, params)

  action perform(cue_ctx, upon_fn) =
    choreo::perform(cue_ctx, upon_fn, apply_custom_effect)

  action step_with(node: Node, listener: LocalContext => Set[Transition]): bool =
    choreo::step_with(node, listener, apply_custom_effect)
}
