// -*- mode: Bluespec; -*-

// Emerald Blockchain Application - Choreo Specification
//
// This specification models the Emerald blockchain application layer that
// integrates MalachiteBFT consensus with an Ethereum execution engine.
//
// Based on: app/src/app.rs
//
// High-level modeling approach:
// - Models multiple app nodes receiving messages from consensus
// - Focuses on state transitions and block lifecycle
// - Abstracts away: streaming details, storage implementation, metrics

module emerald {
  import basicSpells.* from "spells/basicSpells"
  import rareSpells.* from "spells/rareSpells"

  // =============================================================================
  // PROTOCOL CONFIGURATION
  // =============================================================================

  pure val NODES = List("node1", "node2", "node3")

  import choreo(processes = NODES.toSet()) as choreo from "choreo"

  // =============================================================================
  // PROTOCOL-SPECIFIC TYPES
  // =============================================================================

  // Height in the blockchain
  type Height = int

  // Round number within a height
  type Round = int

  // Execution payload
  type Payload = int

  // App current node's phase
  type AppPhase =
    | Uninitialized // Before ConsensusReady
    | Ready         // Between ConsensusReady and StartedRound
    | Working       // After StartedRound
    | Timedout      // After ConsensusTimeout

  // A consensus proposal record
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
  type Event = ()
  type Message = ()

  // Global state recording
  type CustomEffects =
    | RecordProposal(Proposal)
    | RecordDecision(Proposal)
    | RecordAction(ActionTaken)

  // Record information made available for Quint Connect (MBT).
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

  // Different forms of node failure the service can experience
  type FailureMode =
    | NodeCrash
    | NodeRestart
    | ProcessRestart
    | ConsensusTimeout

  // Local state for each node
  type StateFields = {
    phase: AppPhase,
    // Current consensus state
    current_height: Height,
    current_round: Round,
    // Latest committed block
    latest_block_height: Height,
    latest_block_payload: Option[Payload],
    // Node's known proposals
    proposals: Set[Proposal],
  }

  // Global state for inter-node communication and MBT values
  type Extensions = {
    // Track undecided proposals
    pending_proposals: Set[Proposal],
    // Track decided proposals
    decided_proposals: Set[Proposal],
    // Track the action taken for MBT
    action_taken: ActionTaken,
    // Track the set of node falures
    failures: Set[{
      node: Node,
      height: Height,
      round: Round,
      mode: FailureMode
    }]
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

  pure def proposer_for(height: Height, round: Round): Node =
    NODES[(height - 1 + round) % NODES.length()]

  pure def next_payload(ctx: LocalContext): Payload =
    ctx.extensions.pending_proposals.size()

  pure def get_local_proposal(s: LocalState, height: Height, round: Round): Option[Proposal] =
    s.proposals.find(p => p.height == height and p.round == round)

  pure def get_global_decision(ctx: LocalContext, height: Height): Option[Proposal] =
    ctx.extensions.decided_proposals.find(p => p.height == height)

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

  pure def listen_consensus_ready(ctx: LocalContext): Set[()] =
    if (ctx.state.phase == Uninitialized) Set(()) else Set()

  pure def handle_consensus_ready(ctx: LocalContext, _params: ()): Transition = {
    val s = ctx.state

    {
      post_state: {
        ...s,
        phase: Ready,
        current_height: s.latest_block_height + 1,
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

  // TODO: model timeouts that move to next height without deciding on a proposal
  pure def listen_started_round(ctx: LocalContext): Set[(Height, Round)] = {
    val s = ctx.state

    match s.phase {
      | Ready    => Set((s.current_height, s.current_round))
      | Timedout => Set((s.current_height, s.current_round + 1))
      | _        => Set()
    }
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

  pure def listen_get_value(ctx: LocalContext): Set[()] = {
    val s = ctx.state
    val proposer = proposer_for(s.current_height, s.current_round)
    if (s.phase == Working and s.process_id == proposer) Set(()) else Set()
  }

  pure def handle_get_value(ctx: LocalContext, _params: ()): Transition = {
    val s = ctx.state

    // Check if we already have a proposal for this height/round
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
      ),
    }
  }

  // ===== ReceivedProposal: Receive a proposal =====

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

  pure def listen_decided(ctx: LocalContext): Set[Proposal] = {
    val s = ctx.state

    if (s.phase == Working)
      match get_global_decision(ctx, s.current_height) {
        | Some(p) =>
            // Replicates a decision
            if (and {
              p.in(s.proposals),
              p.height > s.latest_block_height,
              p.height == s.current_height,
              p.round == s.current_round
            })
              Set(p)
            else
              Set()

        | None =>
            // Mimics consensus by having the proposer decide
            match get_local_proposal(s, s.current_height, s.current_round) {
              | Some(p) => if (p.proposer == s.process_id) Set(p) else Set()
              | None    => Set()
            }
      }
    else
      Set()
  }

  pure def handle_decided(ctx: LocalContext, decided: Proposal): Transition = {
    val s = ctx.state

    {
      post_state: {
        ...s,
        // Start next height
        phase: Ready,
        current_height: decided.height + 1,
        current_round: 0,
        // Record decision
        latest_block_height: decided.height,
        latest_block_payload: Some(decided.payload),
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

  pure def listen_process_synced_value(ctx: LocalContext): Set[Proposal] = {
    val s = ctx.state

    if (s.phase == Working)
      ctx.extensions.decided_proposals.filterMap(p => {
        if (p.height >= s.current_height and not(p.in(s.proposals)))
          Some(p)
        else
          None
      })
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
            if (s.latest_block_height >= p.height)
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

  // TODO: Implement GetHistoryMinHeight
  // TODO: Implement RestreamProposal

  // ===== NodeCrash: A node crashes and comes back without any state =====

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

  pure def listen_timeout(ctx: LocalContext): Set[()] = {
    val s = ctx.state
    if (s.latest_block_height < s.current_height) Set(()) else Set()
  }

  pure def consensus_timeout(ctx: LocalContext, _params: ()): Transition = {
    val s = ctx.state

    {
      post_state: {
        ...s,
        phase: Timedout
      },
      effects: Set(
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

      // Block building and proposal
      choreo::cue(ctx, listen_get_value, handle_get_value),
      choreo::cue(ctx, listen_received_proposal, handle_received_proposal),

      // Consensus decision
      choreo::cue(ctx, listen_decided, handle_decided),

      // Synchronization protocol
      choreo::cue(ctx, listen_process_synced_value, handle_process_synced_value),
      choreo::cue(ctx, listen_get_decided_value, handle_get_decided_value),

      // Node and process failures
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

  pure def initialize(node: Node): LocalState = {
    {
      process_id: node,
      phase: Uninitialized,
      current_height: 0,
      current_round: 0,
      proposals: Set(),
      // genesis height and payload
      latest_block_height: 0,
      latest_block_payload: None
    }
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

  // =============================================================================
  // PROPERTIES AND INVARIANTS
  // =============================================================================

  // Safety: All nodes agree on decided blocks
  val block_agreement =
    NODES.toSet().forall(n1 => {
      NODES.toSet().forall(n2 => {
        val ctx1 = choreo::convert_context(choreo::s, n1)
        val ctx2 = choreo::convert_context(choreo::s, n2)

        val min_decided_height = min(
          ctx1.state.latest_block_height,
          ctx2.state.latest_block_height
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

  action step_with_filter(f: (LocalContext, Transition) => bool): bool =
    choreo::step(ctx => main_listener(ctx).filter(t => f(ctx, t)), apply_custom_effect)

}
