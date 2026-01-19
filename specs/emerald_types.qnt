// -*- mode: Bluespec; -*-

// =============================================================================
// Emerald Type Definitions
// =============================================================================
//
// This specification contains type definitions used across various Emerald
// specifications.

module emerald_types {
  import basicSpells.* from "spells/basicSpells"

  // =============================================================================
  // PROTOCOL-SPECIFIC TYPES
  // =============================================================================

  // A consensus height
  type Height = int

  // A consensus round within a given height
  type Round = int

  // A block's payload
  type Payload = int

  // The application's current phase.
  //
  // These phases model different stages in the application lifecycle. Emerald
  // nodes start "Uninitialized", then move to "Ready" once Malachite starts
  // (ConsensusReady), and from "Ready" to "Working" after Malachite informs
  // Emerald of a new round starting (StartedRound).
  //
  // During normal operation, Emerald moves back to "Ready" at the end of each
  // consensus height (Decided), then back to "Working" once the new height
  // starts (StartedRound).
  type AppPhase =
    | Uninitialized // Before ConsensusReady
    | Ready         // After ConsensusReady or Decided, before StartedRound
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
  // those are Malachite's responsibility.
  type Message = ()

  type Event =
    // Consensus timeouts are modeled as events, which in return trigger the
    // start of new rounds. (see listen_started_round).
    | Timeout({ height: Height, round: Round })

  type CustomEffects =
    // Records the existence of a new proposal.
    | RecordProposal(Proposal)
    // Records the commit of a given proposal.
    | RecordDecision((Node, Proposal))
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
    | Failure({ node: Node, mode: FailureMode, height: Height })

  // Different forms of node failure the service can experience.
  //
  // Note that NodeRestart and ProcessRestart are the same failure wrt. the
  // model, but MBT distinguishes them by restarting the process or the node
  // when testing.
  type FailureMode =
    // The whole node (all processes) has crashed and lost its data
    | NodeCrash
    // The whole node (all processes) has restarted without data loss
    | NodeRestart
    // The Emerald process has restarted without data loss
    | ProcessRestart
    // Consensus has timed out.
    | ConsensusTimeout

  // Local state for each Emerald node
  type StateFields = {
    phase: AppPhase,
    // Consensus state
    consensus_height: Height,
    consensus_round: Round,
    // Last committed block
    last_decided_height: Height,
    last_decided_payload: Option[Payload],
    // Node's known proposals
    proposals: Set[Proposal],
  }

  // Global state for inter-node communication and MBT
  type Extensions = {
    // Track all proposals
    all_proposals: Set[Proposal],
    // Track decided proposals
    decided_proposals: Set[Proposal],
    // Track node's last decided height
    last_decided_height: Node -> Height,
    // Track the action taken for MBT
    action_taken: ActionTaken,
    // Track the set of failures that occurred
    failures: Set[{node: Node, height: Height, mode: FailureMode}]
  }
}
