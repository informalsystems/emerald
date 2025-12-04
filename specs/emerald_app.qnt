// -*- mode: Bluespec; -*-
/**
 * Emerald Blockchain Application - Choreo Specification
 *
 * This specification models the Emerald blockchain application layer that integrates
 * MalachiteBFT consensus with an Ethereum execution engine.
 *
 * Based on: /home/gabriela/projects/emerald/app/src/app.rs
 *
 * High-level modeling approach:
 * - Models multiple app nodes receiving messages from consensus (nondeterministically)
 * - Focuses on state transitions and block lifecycle
 * - Abstracts away: streaming details, storage implementation, metrics
 * - Includes synchronization protocol for catch-up
 */

module emerald_app {
  import basicSpells.* from "spells/basicSpells"

  // =============================================================================
  // PROTOCOL CONFIGURATION (must be defined before import)
  // =============================================================================

  // Three app nodes
  pure val NODES = Set("node1", "node2", "node3")

  import choreo(processes = NODES) as choreo from "choreo"

  // =============================================================================
  // PROTOCOL-SPECIFIC TYPES
  // =============================================================================

  // Based on state.rs:76-109
  // Node addresses (simplified from complex Address type)
  type Address = str

  // Height in the blockchain (from state.rs:87)
  type Height = int

  // Round number within a height (from state.rs:88)
  type Round = int

  // Block identifier (simplified from BlockHash)
  type BlockHash = str

  // Value identifier for proposals (height, round)
  type ValueId = (Height, Round)

  // Execution payload (abstracted as string for spec purposes)
  type ExecutionPayload = str

  // Validator set (simplified)
  type ValidatorSet = Set[Address]

  // Validity status for proposals (from app.rs:231, 526)
  type Validity = Valid | Invalid

  // Proposal storage states
  type ProposalStatus =
    | Pending        // Future proposal waiting for its height
    | Undecided      // Current proposal being considered
    | DecidedStatus  // Committed with certificate

  // App operational phase
  type AppPhase =
    | Uninitialized   // Before ConsensusReady (validator_set empty, height=0)
    | Ready           // Not used in practice - goes straight to Operating
    | Operating       // After ConsensusReady (height>=1, validator_set initialized)

  // Sync status for catching up
  type SyncStatus =
    | NotSyncing      // Normal operation
    | CatchingUp      // Receiving synced values from peers
    | Providing       // Providing history to other peers

  // Proposal record (simplified from ProposedValue in state.rs)
  type Proposal = {
    height: Height,
    round: Round,
    proposer: Address,
    value_id: ValueId,
    payload: ExecutionPayload,
    validity: Validity,
    status: ProposalStatus
  }

  // =============================================================================
  // MANDATORY TYPE DEFINITIONS
  // =============================================================================

  // Node identifier (app node address)
  type Node = Address

  // Messages from consensus to app (based on app.rs:148-797)
  type Message =
    // Initialize system (app.rs:152)
    | ConsensusReady
    // New consensus round started (app.rs:191)
    | StartedRound((Height, Round, Address))
    // Consensus requests value to propose (app.rs:287)
    | GetValue((Height, Round))
    // Received proposal from network (app.rs:381)
    // Abstracted: in reality, this is streaming parts, we model as complete proposal
    | ReceivedProposal((Node, Proposal))
    // Consensus decided on value (app.rs:414)
    | DecidedMessage((Height, Round, ValueId))
    // Process synced value when catching up (app.rs:597)
    | ProcessSyncedValue((Height, Round, Address, ExecutionPayload))
    // Peer requests decided value for sync (app.rs:706)
    | GetDecidedValue((Node, Height))
    // Query earliest available height (app.rs:729)
    | GetHistoryMinHeight(Node)

  // Local state for each app node (based on state.rs:76-109)
  type StateFields = {
    phase: AppPhase,

    // Current consensus state
    current_height: Height,
    current_round: Round,
    current_proposer: Option[Address],

    // Latest committed block
    latest_block_height: Height,
    latest_block_hash: Option[BlockHash],

    // Validator set (updated from contract after each block)
    validator_set: ValidatorSet,

    // Proposals in various states
    proposals: Set[Proposal],

    // Sync state
    sync_status: SyncStatus,
    min_history_height: Height
  }

  // Custom effects for updating global extensions
  type CustomEffects = UpdateExtensions(Extensions)

  // No external events for this model
  type Event = ()

  // Action types for MBT testing
  type ActionTaken =
    | InitAction
    | ConsensusReadyAction({ node: Node })
    | StartedRoundAction({ node: Node, height: Height, round: Round, proposer: Address })
    | GetValueAction({ node: Node, height: Height, round: Round })
    | ReceivedProposalAction({ node: Node, from: Node, proposal: Proposal })
    | DecidedMessageAction({ node: Node, height: Height, round: Round, value_id: ValueId })
    | ProcessSyncedValueAction({ node: Node, height: Height, round: Round, proposer: Address, payload: ExecutionPayload })
    | GetDecidedValueAction({ node: Node, from: Node, height: Height })
    | GetHistoryMinHeightAction({ node: Node, from: Node })

  // Global extensions to track inter-node communication
  type Extensions = {
    // Track which nodes have which decided values (for sync)
    decided_values: Node -> Set[(Height, ValueId, ExecutionPayload)],
    // Track the action taken for MBT testing
    actionTaken: ActionTaken
  }

  // =============================================================================
  // BOILERPLATE
  // =============================================================================
  type LocalState = choreo::LocalState[Node, StateFields]
  type LocalContext = choreo::LocalContext[Node, StateFields, Message, Event, Extensions]
  type Transition = choreo::Transition[Node, StateFields, Message, Event, CustomEffects]
  type GlobalContext = choreo::GlobalContext[Node, StateFields, Message, Event, Extensions]

  // =============================================================================
  // HELPER FUNCTIONS
  // =============================================================================

  // Check if a proposal exists in a given status
  pure def has_proposal(proposals: Set[Proposal], h: Height, r: Round, status: ProposalStatus): bool = {
    proposals.exists(p => p.height == h and p.round == r and p.status == status)
  }

  // Get proposal by height, round, and value_id
  pure def get_proposal(proposals: Set[Proposal], h: Height, r: Round, vid: ValueId): Option[Proposal] = {
    proposals.filter(p => p.height == h and p.round == r and p.value_id == vid).fold(None, (_, p) => Some(p))
  }

  // Get undecided proposal for a height/round
  pure def get_undecided_proposal(proposals: Set[Proposal], h: Height, r: Round): Option[Proposal] = {
    proposals.filter(p => p.height == h and p.round == r and p.status == Undecided).fold(None, (_, p) => Some(p))
  }

  // Check if node is proposer for this round
  pure def is_proposer(node: Node, proposer: Address): bool = {
    node == proposer
  }

  // Abstract validation function (models execution engine validation)
  // In real code: engine.notify_new_block(...) -> PayloadStatus (app.rs:521-533)
  pure def validate_payload(payload: ExecutionPayload): Validity = {
    // For spec purposes, all payloads are valid (abstracting execution engine)
    // In reality, this would be determined by execution engine
    Valid
  }

  // Generate a new proposal (models building block from execution engine)
  // In real code: engine.generate_block(...) (app.rs:337-339)
  pure def generate_proposal(height: Height, round: Round, proposer: Address): Proposal = {
    {
      height: height,
      round: round,
      proposer: proposer,
      value_id: (height, round),
      payload: "payload",  // Abstracted for spec
      validity: Valid,  // Own proposals are valid
      status: Undecided
    }
  }

  // Abstract query to validator contract (models read_validators_from_contract)
  // In real code: read_validators_from_contract(...) (app.rs:104-140)
  pure def query_validators(): ValidatorSet = {
    NODES  // Simplified: all nodes are validators
  }

  // =============================================================================
  // TRANSITION FUNCTIONS
  // =============================================================================

  // ===== ConsensusReady: Initialize system (app.rs:152-187) =====

  pure def listen_consensus_ready(ctx: LocalContext): Set[bool] = {
    val s = ctx.state
    val messages = ctx.messages

    val state_guard = s.phase == Uninitialized
    val has_msg = messages.exists(m => match m {
      | ConsensusReady => true
      | _ => false
    })

    if (state_guard and has_msg) Set(true) else Set()
  }

  pure def handle_consensus_ready(ctx: LocalContext, _u: bool): Transition = {
    val s = ctx.state
    val extensions = ctx.extensions

    // Initialize from genesis (app.rs:33-48, 173-177)
    // The implementation:
    // 1. Gets genesis block from execution engine (height 0)
    // 2. Sets state.latest_block = genesis_block
    // 3. Reads validators from ValidatorManager contract
    // 4. Sets state.current_height = Height::default() = 1
    //
    // So consensus starts at height 1 to build the first block,
    // with genesis block (height 0) as the latest committed block.
    val validators = NODES  // Simplified: all nodes are validators

    {
      effects: Set(choreo::CustomEffect(UpdateExtensions({
        ...extensions,
        actionTaken: ConsensusReadyAction({ node: s.process_id })
      }))),
      post_state: {
        ...s,
        phase: Operating,  // Operating because current_height > 0
        current_height: 1,  // Height::default() = 1, ready to build first block
        current_round: 0,
        latest_block_height: 0,  // Genesis block is at height 0
        latest_block_hash: Some("genesis"),  // Genesis block hash (abstracted)
        validator_set: validators
      }
    }
  }

  // ===== StartedRound: Enter new round (app.rs:191-283) =====

  pure def listen_started_round(ctx: LocalContext): Set[(Height, Round, Address)] = {
    val s = ctx.state
    val messages = ctx.messages

    val state_guard = s.phase == Ready or s.phase == Operating

    val started_msgs = messages.filterMap(m => match m {
      | StartedRound(params) => Some(params)
      | _ => None
    })

    if (state_guard) started_msgs else Set()
  }

  pure def handle_started_round(ctx: LocalContext, params: (Height, Round, Address)): Transition = {
    val s = ctx.state
    val extensions = ctx.extensions
    val height = params._1
    val round = params._2
    val proposer = params._3

    // Move pending proposals to undecided if they match current height
    // (app.rs:277-282)
    val updated_proposals = s.proposals.map(p =>
      if (p.status == Pending and p.height == height and p.round == round) {
        { ...p, status: Undecided }
      } else {
        p
      }
    )

    {
      effects: Set(choreo::CustomEffect(UpdateExtensions({
        ...extensions,
        actionTaken: StartedRoundAction({ node: s.process_id, height: height, round: round, proposer: proposer })
      }))),
      post_state: {
        ...s,
        phase: Operating,
        current_height: height,
        current_round: round,
        current_proposer: Some(proposer),
        proposals: updated_proposals
      }
    }
  }

  // ===== GetValue: Build and propose new value (app.rs:287-374) =====

  pure def listen_get_value(ctx: LocalContext): Set[(Height, Round)] = {
    val s = ctx.state
    val messages = ctx.messages

    val state_guard = s.phase == Operating

    val get_value_msgs = messages.filterMap(m => match m {
      | GetValue(params) => Some(params)
      | _ => None
    })

    // Only respond if we are the proposer
    val filtered = get_value_msgs.filter(hr =>
      val h = hr._1
      val r = hr._2
      match s.current_proposer {
        | Some(prop) => is_proposer(s.process_id, prop) and h == s.current_height and r == s.current_round
        | None => false
      }
    )

    if (state_guard) filtered else Set()
  }

  pure def handle_get_value(ctx: LocalContext, params: (Height, Round)): Transition = {
    val s = ctx.state
    val extensions = ctx.extensions
    val height = params._1
    val round = params._2

    // Check if we already have a proposal for this height/round (app.rs:301-316)
    val existing = get_undecided_proposal(s.proposals, height, round)

    match existing {
      | Some(proposal) => {
        // Re-use existing proposal
        {
          effects: Set(
            // Broadcast proposal to network (abstracted)
            choreo::Broadcast(ReceivedProposal((s.process_id, proposal))),
            choreo::CustomEffect(UpdateExtensions({
              ...extensions,
              actionTaken: GetValueAction({ node: s.process_id, height: height, round: round })
            }))
          ),
          post_state: s
        }
      }
      | None => {
        // Generate new proposal (app.rs:331-349)
        val new_proposal = generate_proposal(height, round, s.process_id)

        {
          effects: Set(
            // Broadcast proposal to network (app.rs:366-372)
            choreo::Broadcast(ReceivedProposal((s.process_id, new_proposal))),
            choreo::CustomEffect(UpdateExtensions({
              ...extensions,
              actionTaken: GetValueAction({ node: s.process_id, height: height, round: round })
            }))
          ),
          post_state: {
            ...s,
            proposals: s.proposals.union(Set(new_proposal))
          }
        }
      }
    }
  }

  // ===== ReceivedProposal: Receive and validate proposal (app.rs:381-407) =====

  pure def listen_received_proposal(ctx: LocalContext): Set[(Node, Proposal)] = {
    val s = ctx.state
    val messages = ctx.messages

    val state_guard = s.phase == Operating

    val proposal_msgs = messages.filterMap(m => match m {
      | ReceivedProposal(params) => Some(params)
      | _ => None
    })

    // Filter: only accept proposals we haven't seen and are for current or future height
    val filtered = proposal_msgs.filter(fp =>
      val from = fp._1
      val prop = fp._2
      prop.height >= s.current_height and
      not(s.proposals.exists(p => p.value_id == prop.value_id)) and
      from != s.process_id  // Don't process our own proposals
    )

    if (state_guard) filtered else Set()
  }

  pure def handle_received_proposal(ctx: LocalContext, params: (Node, Proposal)): Transition = {
    val s = ctx.state
    val extensions = ctx.extensions
    val from = params._1
    val proposal = params._2

    // Determine proposal status based on height (app.rs:400-419)
    val status = if (proposal.height > s.current_height) {
      Pending  // Future proposal (app.rs:416-418)
    } else if (proposal.height == s.current_height) {
      // Validate proposal (app.rs:422-475)
      // Check proposer is correct and signature valid (abstracted)
      // Then validate execution payload (app.rs:438-447)
      val validity = validate_payload(proposal.payload)
      if (validity == Valid) Undecided else Pending  // Invalid proposals ignored
    } else {
      Pending  // Outdated, ignore (app.rs:401-411)
    }

    val validated_proposal = { ...proposal, status: status, validity: validate_payload(proposal.payload) }

    // Only store if valid
    val should_store = validated_proposal.validity == Valid

    {
      effects: Set(choreo::CustomEffect(UpdateExtensions({
        ...extensions,
        actionTaken: ReceivedProposalAction({ node: s.process_id, from: from, proposal: proposal })
      }))),
      post_state: if (should_store) {
        {
          ...s,
          proposals: s.proposals.union(Set(validated_proposal))
        }
      } else {
        s
      }
    }
  }

  // ===== Decided: Commit decided value (app.rs:414-589) =====

  pure def listen_decided(ctx: LocalContext): Set[(Height, Round, ValueId)] = {
    val s = ctx.state
    val messages = ctx.messages

    val state_guard = s.phase == Operating

    val decided_msgs = messages.filterMap(m => match m {
      | DecidedMessage(params) => Some(params)
      | _ => None
    })

    if (state_guard) decided_msgs else Set()
  }

  pure def handle_decided(ctx: LocalContext, params: (Height, Round, ValueId)): Transition = {
    val s = ctx.state
    val extensions = ctx.extensions
    val ext = ctx.extensions
    val height = params._1
    val round = params._2
    val value_id = params._3

    // Get the decided proposal (app.rs:425-434)
    val proposal_opt = get_proposal(s.proposals, height, round, value_id)

    match proposal_opt {
      | Some(proposal) => {
        // Mark as decided
        val decided_proposal = { ...proposal, status: DecidedStatus }

        // Update proposals: remove old, add decided
        val updated_proposals = s.proposals.exclude(Set(proposal)).union(Set(decided_proposal))

        // Generate new block hash (app.rs:436)
        val new_block_hash = "block"  // Abstracted for spec

        // Query new validator set (app.rs:573-577)
        val new_validators = NODES  // Simplified: all nodes are validators

        // Record decided value in extensions for sync protocol
        val updated_extensions = {
          ...ext,
          decided_values: ext.decided_values.set(
            s.process_id,
            ext.decided_values.get(s.process_id).union(Set((height, value_id, proposal.payload)))
          ),
          actionTaken: DecidedMessageAction({ node: s.process_id, height: height, round: round, value_id: value_id })
        }

        {
          effects: Set(
            choreo::CustomEffect(UpdateExtensions(updated_extensions))
          ),
          post_state: {
            ...s,
            proposals: updated_proposals,
            latest_block_height: height,
            latest_block_hash: Some(new_block_hash),
            validator_set: new_validators,
            // Move to next height (app.rs:579-580)
            current_height: height + 1,
            current_round: 0
          }
        }
      }
      | None => {
        // No proposal found - should not happen in correct execution
        // (app.rs:524-532)
        {
          effects: Set(choreo::CustomEffect(UpdateExtensions({
            ...extensions,
            actionTaken: DecidedMessageAction({ node: s.process_id, height: height, round: round, value_id: value_id })
          }))),
          post_state: s
        }
      }
    }
  }

  // ===== ProcessSyncedValue: Process synced block when catching up (app.rs:597-699) =====

  pure def listen_process_synced_value(ctx: LocalContext): Set[(Height, Round, Address, ExecutionPayload)] = {
    val s = ctx.state
    val messages = ctx.messages

    val state_guard = s.phase == Operating

    val synced_msgs = messages.filterMap(m => match m {
      | ProcessSyncedValue(params) => Some(params)
      | _ => None
    })

    if (state_guard) synced_msgs else Set()
  }

  pure def handle_process_synced_value(ctx: LocalContext, params: (Height, Round, Address, ExecutionPayload)): Transition = {
    val s = ctx.state
    val extensions = ctx.extensions
    val height = params._1
    val round = params._2
    val proposer = params._3
    val payload = params._4

    // Validate synced payload (app.rs:632-641)
    val validity = validate_payload(payload)

    if (validity == Valid) {
      // Create proposal from synced value (app.rs:665-672)
      val synced_proposal = {
        height: height,
        round: round,
        proposer: proposer,
        value_id: (height, round),
        payload: payload,
        validity: Valid,
        status: Undecided
      }

      {
        effects: Set(choreo::CustomEffect(UpdateExtensions({
          ...extensions,
          actionTaken: ProcessSyncedValueAction({ node: s.process_id, height: height, round: round, proposer: proposer, payload: payload })
        }))),
        post_state: {
          ...s,
          proposals: s.proposals.union(Set(synced_proposal)),
          sync_status: CatchingUp
        }
      }
    } else {
      // Reject invalid synced value (app.rs:643-659)
      {
        effects: Set(choreo::CustomEffect(UpdateExtensions({
          ...extensions,
          actionTaken: ProcessSyncedValueAction({ node: s.process_id, height: height, round: round, proposer: proposer, payload: payload })
        }))),
        post_state: s
      }
    }
  }

  // ===== GetDecidedValue: Provide decided value for sync (app.rs:706-725) =====

  pure def listen_get_decided_value(ctx: LocalContext): Set[(Node, Height)] = {
    val s = ctx.state
    val messages = ctx.messages

    val state_guard = s.phase == Operating

    val get_decided_msgs = messages.filterMap(m => match m {
      | GetDecidedValue(params) => Some(params)
      | _ => None
    })

    if (state_guard) get_decided_msgs else Set()
  }

  pure def handle_get_decided_value(ctx: LocalContext, params: (Node, Height)): Transition = {
    val s = ctx.state
    val extensions = ctx.extensions
    val ext = ctx.extensions
    val from = params._1
    val height = params._2

    // Check if we have decided value at this height (app.rs:711-720)
    val decided_values = ext.decided_values.get(s.process_id)
    val matching_value = decided_values.filter(dv => dv._1 == height).fold(None, (_, dv) => Some(dv))

    match matching_value {
      | Some(dv) => {
        val h = dv._1
        val vid = dv._2
        val payload = dv._3
        // Send synced value to requesting node
        {
          effects: Set(
            choreo::Send({
              to: from,
              message: ProcessSyncedValue((h, 0, s.process_id, payload))
            }),
            choreo::CustomEffect(UpdateExtensions({
              ...extensions,
              actionTaken: GetDecidedValueAction({ node: s.process_id, from: from, height: height })
            }))
          ),
          post_state: {
            ...s,
            sync_status: Providing
          }
        }
      }
      | None => {
        // No decided value at this height (app.rs:718-720)
        {
          effects: Set(choreo::CustomEffect(UpdateExtensions({
            ...extensions,
            actionTaken: GetDecidedValueAction({ node: s.process_id, from: from, height: height })
          }))),
          post_state: s
        }
      }
    }
  }

  // ===== GetHistoryMinHeight: Report earliest available height (app.rs:729-735) =====

  pure def listen_get_history_min_height(ctx: LocalContext): Set[Node] = {
    val s = ctx.state
    val messages = ctx.messages

    val state_guard = s.phase == Operating

    val min_height_msgs = messages.filterMap(m => match m {
      | GetHistoryMinHeight(from) => Some(from)
      | _ => None
    })

    if (state_guard) min_height_msgs else Set()
  }

  pure def handle_get_history_min_height(ctx: LocalContext, from: Node): Transition = {
    val s = ctx.state
    val extensions = ctx.extensions

    // Report minimum available height (app.rs:730)
    // In spec, we just update state to track that we provided info
    {
      effects: Set(choreo::CustomEffect(UpdateExtensions({
        ...extensions,
        actionTaken: GetHistoryMinHeightAction({ node: s.process_id, from: from })
      }))),
      post_state: s
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

      // Block building and proposal
      choreo::cue(ctx, listen_get_value, handle_get_value),
      choreo::cue(ctx, listen_received_proposal, handle_received_proposal),

      // Consensus decision
      choreo::cue(ctx, listen_decided, handle_decided),

      // Synchronization protocol
      choreo::cue(ctx, listen_process_synced_value, handle_process_synced_value),
      choreo::cue(ctx, listen_get_decided_value, handle_get_decided_value),
      choreo::cue(ctx, listen_get_history_min_height, handle_get_history_min_height)
    ).flatten()
  }

  // =============================================================================
  // CUSTOM EFFECT PROCESSOR
  // =============================================================================

  pure def apply_custom_effect(ctx: GlobalContext, effect: CustomEffects): GlobalContext = {
    match effect {
      | UpdateExtensions(new_ext) => { ...ctx, extensions: new_ext }
    }
  }

  // =============================================================================
  // (NODES already defined above before import)
  // =============================================================================

  // =============================================================================
  // INITIALIZATION
  // =============================================================================

  pure def initialize(n: Node): LocalState = {
    {
      process_id: n,
      phase: Uninitialized,
      current_height: 0,
      current_round: 0,
      current_proposer: None,
      latest_block_height: 0,
      latest_block_hash: None,
      validator_set: Set(),
      proposals: Set(),
      sync_status: NotSyncing,
      min_history_height: 0
    }
  }

  action init = choreo::init({
    system: NODES.mapBy(n => initialize(n)),
    messages: NODES.mapBy(n => Set()),
    events: NODES.mapBy(n => Set()),
    extensions: {
      decided_values: NODES.mapBy(n => Set()),
      actionTaken: InitAction
    }
  })

  // =============================================================================
  // STEP FUNCTION
  // =============================================================================

  // Basic step: just process messages
  action appStep = choreo::step(main_listener, apply_custom_effect)

  // Combined step: can inject messages OR process them
  action step = any {
    // App processes messages
    appStep,

    // Consensus can inject messages nondeterministically
    injectMessageAll(ConsensusReady),

    // Inject StartedRound with node1 as proposer
    injectMessageAll(StartedRound((1, 0, "node1"))),

    // Inject StartedRound with node2 as proposer
    injectMessageAll(StartedRound((1, 0, "node2"))),

    // Inject StartedRound with node3 as proposer
    injectMessageAll(StartedRound((1, 0, "node3"))),

    // Inject GetValue to node1
    injectMessage("node1", GetValue((1, 0))),

    // Inject GetValue to node2
    injectMessage("node2", GetValue((1, 0))),

    // Inject GetValue to node3
    injectMessage("node3", GetValue((1, 0))),

    // Inject DecidedMessage
    injectMessageAll(DecidedMessage((1, 0, (1, 0))))
  }

  // =============================================================================
  // PROPERTIES AND INVARIANTS
  // =============================================================================

  // Safety: All nodes agree on decided blocks at the same height
  val block_agreement = NODES.forall(n1 => {
    NODES.forall(n2 => {
      val state1 = choreo::s.system.get(n1)
      val state2 = choreo::s.system.get(n2)

      // If both have decided a block at the same height, they must agree on the hash
      state1.latest_block_height == state2.latest_block_height implies
        state1.latest_block_hash == state2.latest_block_hash
    })
  })

  // Validity: Decided proposals must be valid
  val decided_validity = NODES.forall(n => {
    val state = choreo::s.system.get(n)
    state.proposals.forall(p =>
      p.status == DecidedStatus implies p.validity == Valid
    )
  })

  // Progress: Height only increases
  val monotonic_height = NODES.forall(n => {
    val state = choreo::s.system.get(n)
    state.current_height >= state.latest_block_height
  })

  // =============================================================================
  // TESTING AND DEBUGGING
  // =============================================================================

  // =============================================================================
  // TEST SCENARIOS WITH MESSAGE INJECTION
  // =============================================================================

  // Helper: try to process messages, don't fail if none available
  action tryStep = any {
    appStep,                         // Try to process
    all { choreo::s' = choreo::s }  // Or succeed without processing (no messages)
  }

  // Debug test: Check exact pattern from singleHeightConsensus
  run debugPattern = init
    .then(injectMessage("node1", ConsensusReady))
    .then(injectMessage("node2", ConsensusReady))
    .then(injectMessage("node3", ConsensusReady))
    .then(tryStep).then(tryStep).then(tryStep).then(tryStep)
    .expect(NODES.forall(n => getAppPhase(n) == Ready))

  // Debug test: Check all nodes can become ready
  run debugConsensusReady = init
    .then(injectMessage("node1", ConsensusReady))
    .then(injectMessage("node2", ConsensusReady))
    .then(injectMessage("node3", ConsensusReady))
    .then(appStep)  // Process one
    .then(appStep)  // Process another
    .then(appStep)  // Process third
    .expect(NODES.forall(n => getAppPhase(n) == Ready))  // All should be Ready

  // Test Scenario 1: Single Height Consensus - Basic smoke test
  // Just verify the basic protocol flow works
  run singleHeightConsensus = init
    .then(injectMessageAll(ConsensusReady))
    .then(3.reps(_ => tryStep))
    .then(injectMessageAll(StartedRound((1, 0, "node1"))))
    .then(3.reps(_ => tryStep))
    .then(injectMessage("node1", GetValue((1, 0))))
    .then(3.reps(_ => tryStep))
    .then(injectMessageAll(DecidedMessage((1, 0, (1, 0)))))
    .then(3.reps(_ => tryStep))

  // Test Scenario 2: Two Heights (controlled sequence)
  run twoHeightConsensus = singleHeightConsensus
    // Height 2 (different proposer)
    .then(injectMessageAll(StartedRound((2, 0, "node2"))))
    .then(3.reps(_ => tryStep))
    .then(injectMessage("node2", GetValue((2, 0))))
    .then(3.reps(_ => tryStep))
    .then(injectMessageAll(DecidedMessage((2, 0, (2, 0)))))
    .then(3.reps(_ => tryStep))

    // .expect(all {
    //   block_agreement,
    //   decided_validity,
    //   monotonic_height,

    //   // All nodes at height 3
    //   NODES.forall(n => getAppHeight(n) == 3)
    // })

  // Test Scenario 3: Nondeterministic execution with message injection
  run nondeterministicConsensus = init
    .then(10.reps(_ => step))  // Now step can inject AND process
    .expect(all {
      block_agreement,
      decided_validity,
      monotonic_height
    })

  // =============================================================================
  // WIRING
  // =============================================================================

  val s = choreo::s

  action step_with(v: Node, listener: LocalContext => Set[Transition]): bool =
    choreo::step_with(v, listener, apply_custom_effect)

  action step_deterministic(v: Node, listener: LocalContext => Transition): bool =
    choreo::step_deterministic(v, listener, apply_custom_effect)

  // =============================================================================
  // TEST HELPERS (for external test modules)
  // =============================================================================

  // Inject a message to a specific node
  action injectMessage(node: Node, msg: Message): bool = all {
    choreo::s' = {
      ...choreo::s,
      messages: choreo::s.messages.setBy(node, msgs => msgs.union(Set(msg)))
    }
  }

  // Inject a message to all nodes
  action injectMessageAll(msg: Message): bool = all {
    choreo::s' = {
      ...choreo::s,
      messages: NODES.mapBy(n => Set(msg))
    }
  }

  // Query app state for a node
  def getAppPhase(node: Node): AppPhase = choreo::s.system.get(node).phase
  def getAppHeight(node: Node): Height = choreo::s.system.get(node).current_height
  def getAppRound(node: Node): Round = choreo::s.system.get(node).current_round

  def with_cue(process, listen_fn, params) =
    choreo::with_cue(process, listen_fn, params)

  action perform(cue_ctx, upon_fn) =
    choreo::perform(cue_ctx, upon_fn, apply_custom_effect)
}
