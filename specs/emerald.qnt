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
// network, storage, and blockchain systems are considered a implementation
// details and it's not modeled here.
//
// For model-based testing (MBT), actions and relevant data are tracked as part
// of global state so they can be made available to Quint Connect for testing.

module emerald {
  import basicSpells.* from "spells/basicSpells"
  import rareSpells.* from "spells/rareSpells"
  import choreo(processes = NODES.toSet()) as choreo from "choreo"

  // =============================================================================
  // PROTOCOL CONFIGURATION
  // =============================================================================

  // The Emerald nodes to test. E.g.: List("node1", "node2", "node3")
  const NODES: List[Node]

  // =============================================================================
  // PROTOCOL-SPECIFIC TYPES
  // =============================================================================

  // A consensus height
  type Height = int

  // A consensus round withing a given height
  type Round = int

  // A block's payload
  type Payload = int

  // The application's current phase.
  //
  // These phases model different stages in the application startup process.
  // Emerald nodes start "Uninitialized", then move to "Ready" once Malachite
  // starts (ConsensusReady), and from "Ready" to "Working" after Malachite
  // informs Emerald of a new round starting (StartedRound).
  type AppPhase =
    | Uninitialized // Before ConsensusReady
    | Ready         // Between ConsensusReady and StartedRound
    | Working       // After StartedRound

  // A consensus proposal
  type Proposal = {
    height: Height,
    round: Round,
    proposer: Node,
    payload: Payload,
  }

  // =============================================================================
  // CHOREO AND MBT TYPE DEFINITIONS
  // =============================================================================

  type Node = str

  // Note: this specification does not model message exchanges between nodes as
  // those are Malachaite's responsibility.
  type Message = ()

  type Event =
    // Consensus timeouts are modeled as events, which in return trigger the
    // start of new rounds. (see listen_started_round).
    | Timeout({ height: Height, round: Round })

  type CustomEffects =
    // Records the existence of a new proposal.
    | RecordProposal(Proposal)
    // Records the commit of a given proposal.
    | RecordDecision(Proposal)
    // Records which action was taken by the model for MBT.
    | RecordAction(ActionTaken)

  // Actions taken by the model for MBT.
  //
  // Variants must either have no arguments or a single object as an argument.
  // Object fields become available in the test code using the Quint Connect
  // library.
  type ActionTaken =
    | InitAction
    | ConsensusReadyAction({ node: Node })
    | StartedRoundAction({ node: Node, height: Height, round: Round, proposer: Node })
    | GetValueAction({ node: Node, height: Height, round: Round, proposal: Proposal })
    | ReceivedProposalAction({ node: Node, proposal: Proposal })
    | DecidedAction({ node: Node, proposal: Proposal })
    | ProcessSyncedValueAction({ node: Node, proposal: Proposal })
    | GetDecidedValueAction({ node: Node, height: Height, proposal: Option[Proposal] })
    | Failure({ node: Node, mode: FailureMode })

  // Different forms of node failure the service can experience.
  //
  // Note that NodeRestart and ProcessRestart are the same failure wrt. the
  // model but, MBT distinguishes them by restarting the process or the node
  // when testing.
  type FailureMode =
    // The whole node (all processes) has crash and lost its data.
    | NodeCrash
    // The whole node (all processes) has restarted without data loss.
    | NodeRestart
    // The Emerald process has restarted without data loss.
    | ProcessRestart
    // Consensus has timed out.
    | ConsensusTimeout

  // Local state for each Emerald node
  type StateFields = {
    phase: AppPhase,
    // Consensus state
    current_height: Height,
    current_round: Round,
    // Last committed block
    last_decided_height: Height,
    last_decided_payload: Option[Payload],
    // Node's known proposals
    proposals: Set[Proposal],
  }

  // Global state for inter-node communication and MBT
  type Extensions = {
    // Track undecided proposals
    pending_proposals: Set[Proposal],
    // Track decided proposals
    decided_proposals: Set[Proposal],
    // Track the action taken for MBT
    action_taken: ActionTaken,
    // Track the set of failures that occurred
    failures: Set[{node: Node, height: Height, round: Round, mode: FailureMode}]
  }

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
  // many pending proposals there are, thus ensuring a unique payload per
  // proposal recorded.
  pure def next_payload(ctx: LocalContext): Payload =
    ctx.extensions.pending_proposals.size()

  // Get a proposal from the local state for the given height and round, if any.
  pure def get_local_proposal(s: LocalState, height: Height, round: Round): Option[Proposal] =
    s.proposals.find(p => p.height == height and p.round == round)

  // Get a decided proposal on the global scope for the given height, if any.
  pure def get_global_decision(ctx: LocalContext, height: Height): Option[Proposal] =
    ctx.extensions.decided_proposals.find(p => p.height == height)

  // Given a state transition, return which action was taken by it.
  pure def get_action_taken(t: Transition): ActionTaken =
    t.effects
      .filterMap(eff => match eff {
        | CustomEffect(ceff) => match ceff {
            | RecordAction(act) => Some(act)
            | _                 => None
          }
        | _ => None
      })
      .getOnlyElement()

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

    {
      post_state: {
        ...s,
        phase: Ready,
        current_height: s.last_decided_height + 1,
        current_round: 0
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
  // Assumption: After ConsensusReady, or upon a timeout, Malachite will call
  // StartedRound informing Emerald of a new round starting. In response,
  // Emerald will track the current height and round.

  pure def listen_started_round(ctx: LocalContext): Set[(Height, Round)] = {
    val s = ctx.state

    val timeout =
      ctx.events.find(e => match e {
        | Timeout(t) => and {
            t.height == s.current_height,
            t.round == s.current_round,
          }
      })

    if (timeout != None)       Set((s.current_height, s.current_round + 1))
    else if (s.phase == Ready) Set((s.current_height, s.current_round))
    else                       Set()
  }

  pure def handle_started_round(ctx: LocalContext, params: (Height, Round)): Transition = {
    val s = ctx.state
    val height = params._1
    val round = params._2

    val proposer = proposer_for(height, round)

    {
      post_state: {
        ...s,
        phase: Working,
        current_height: height,
        current_round: round,
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
  // Emerald node that is the proposer for the current height and round. In
  // response, Emerald will either propose a new value or return an already
  // proposed value.

  pure def listen_get_value(ctx: LocalContext): Set[()] = {
    val s = ctx.state
    val proposer = proposer_for(s.current_height, s.current_round)
    if (s.phase == Working and s.process_id == proposer) Set(()) else Set()
  }

  pure def handle_get_value(ctx: LocalContext, _params: ()): Transition = {
    val s = ctx.state

    val proposal =
      match get_local_proposal(s, s.current_height, s.current_round) {
        | Some(proposal) => proposal
        | None => {
            height: s.current_height,
            round: s.current_round,
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
            height: s.current_height,
            round: s.current_round,
            proposal: proposal,
          })
        ))
      )
    }
  }

  // ===== ReceivedProposal: Receive a proposal =====
  //
  // Assumption: Malachite will call ReceivedProposal on the Emerald node
  // informing of a new proposal being received for a height >= current_height.
  // In response, Emerald will store the new proposal in its local state.

  pure def listen_received_proposal(ctx: LocalContext): Set[Proposal] = {
    val s = ctx.state

    if (s.phase == Working)
      ctx.extensions.pending_proposals.filterMap(p => {
        if (p.height >= s.current_height and not(p.in(s.proposals)))
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
  // Assumption: Eventually the proposer decides on the proposal. Malachite
  // calls Decided on an Emerald node to inform of a decision. In response,
  // Emerald tracks decision and moves to the next height.

  pure def listen_decided(ctx: LocalContext): Set[Proposal] = {
    val s = ctx.state

    if (s.phase == Working)
      match get_global_decision(ctx, s.current_height) {
        // There isn't a decision for the current height. Let's mimic Malachite
        // consensus and have the proposer decide on a proposal for the current
        // height and round.
        | None =>
            match get_local_proposal(s, s.current_height, s.current_round) {
              | Some(p) => if (p.proposer == s.process_id) Set(p) else Set()
              | None    => Set()
            }
        // A decision has been made for the current height. Let's replicate the
        // decision as long as the Emerald node is at the same height and round.
        | Some(p) =>
            if (and {
              p.in(s.proposals),
              p.height > s.last_decided_height,
              p.height == s.current_height,
              p.round == s.current_round
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
        current_height: decided.height + 1,
        current_round: 0,
        // Records the last decided block
        last_decided_height: decided.height,
        last_decided_payload: Some(decided.payload),
      },
      effects: Set(
        choreo::CustomEffect(RecordDecision(decided)),
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
  // decided proposals in height order. In response, Emerald tracks the newly
  // received proposals.
  //
  // Note that a later call to Decided is made by Malachite to confirm the
  // received decided proposals.

  pure def listen_process_synced_value(ctx: LocalContext): Set[Proposal] = {
    val s = ctx.state

    if (s.phase == Working)
      val candidate =
        ctx.extensions.decided_proposals
          .filter(p => not(p.in(s.proposals)))
          .fold(None, (acc, b) => match acc {
            | Some(a) => if (a.height < b.height) Some(a) else Some(b)
            | None    => Some(b)
          })

      match candidate {
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

    if (s.phase == Working)
      val max_decided_height =
        ctx.extensions
          .decided_proposals
          .fold(1, (h, p) => max(h, p.height))
      1.to(max_decided_height)
    else
      Set()
  }

  pure def handle_get_decided_value(ctx: LocalContext, height: Height): Transition = {
    val s = ctx.state

    val known_decided =
      match get_global_decision(ctx, height) {
        | None => None
        | Some(p) =>
            if (s.last_decided_height >= p.height)
              Some(p)
            else
              None
      }

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

  // ===== NodeCrash: A node crashes and comes back without any state =====
  //
  // Assumption: A whole node can crash at any point and lose all its data. When
  // restarted, it catches up with others ReceivedProposal or
  // ProcessSyncedValue.

  pure def node_crash(ctx: LocalContext): Set[Transition] = {
    val s = ctx.state

    Set({
      post_state: initialize(s.process_id),
      effects: Set(
        choreo::CustomEffect(RecordAction(
          Failure({
            node: s.process_id,
            mode: NodeCrash
          })
        ))
      )
    })
  }

  // ===== NodeRestart: A node restarts but preserves its state =====
  //
  // Assumption: A node can restart without data loss. When restarted, it comes
  // back at height and round zero, then ConsensusReady and StartedRound will
  // move them back to they height they were before restarting.

  pure def node_restart(ctx: LocalContext): Set[Transition] = {
    val s = ctx.state

    Set({
      post_state: {
        ...s,
        phase: Uninitialized,
        current_height: 0,
        current_round: 0,
      },
      effects: Set(
        choreo::CustomEffect(RecordAction(
          Failure({
            node: s.process_id,
            mode: NodeRestart
          })
        ))
      )
    })
  }

  // ===== ProcessRestart: A process restart restarts but preserves its state =====
  // 
  // Assumption: The Emerald process can restart without data loss. When
  // restarted, it comes back at height and round zero, then ConsensusReady and
  // StartedRound will move them back to they height they were before
  // restarting.

  pure def process_restart(ctx: LocalContext): Set[Transition] = {
    val s = ctx.state

    Set({
      post_state: {
        ...s,
        phase: Uninitialized,
        current_height: 0,
        current_round: 0,
      },
      effects: Set(
        choreo::CustomEffect(RecordAction(
          Failure({
            node: s.process_id,
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
    if (s.last_decided_height < s.current_height) Set(()) else Set()
  }

  pure def consensus_timeout(ctx: LocalContext, _params: ()): Transition = {
    val s = ctx.state

    {
      post_state: s,
      effects: Set(
        choreo::TriggerEvent(Timeout({
          height: s.current_height,
          round: s.current_round
        })),
        choreo::CustomEffect(RecordAction(
          Failure({
            node: s.process_id,
            mode: ConsensusTimeout
          })
        ))
      )
    }
  }

  // =============================================================================
  // MAIN LISTENERS
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
            pending_proposals: ext.pending_proposals.setAdd(proposal)
          }
        }
      | RecordDecision(proposal) => {
          ...ctx,
          extensions: {
            ...ext,
            decided_proposals: ext.decided_proposals.setAdd(proposal)
          }
        }
      | RecordAction(action_taken) =>
          val ext = {
            ...ctx.extensions,
            action_taken: action_taken
          }

          val ext0 = match action_taken {
            | Failure(args) =>
                val s = ctx.system.get(args.node)
                {
                  ...ext,
                  failures: ext.failures.setAdd({
                    node: args.node,
                    height: s.current_height,
                    round: s.current_round,
                    mode: args.mode
                  })
                }
            | _ => ext
          }

          {
            ...ctx,
            extensions: {
              ...ext0,
              action_taken: action_taken
            }
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
      current_height: 0,
      current_round: 0,
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
        pending_proposals: Set(),
        decided_proposals: Set(),
        action_taken: InitAction,
        failures: Set(),
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

  // Safety: All nodes agree on decided blocks
  val agreement =
    NODES.toSet().forall(n1 => {
      NODES.toSet().forall(n2 => {
        val ctx1 = choreo::convert_context(choreo::s, n1)
        val ctx2 = choreo::convert_context(choreo::s, n2)

        val min_decided_height =
          min(
            ctx1.state.last_decided_height,
            ctx2.state.last_decided_height
          )

        1.to(min_decided_height).forall(height =>
          val decided = get_global_decision(ctx1, height).unwrap()
          and {
            decided.in(ctx1.state.proposals),
            decided.in(ctx2.state.proposals)
          }
        )
      })
    })

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
