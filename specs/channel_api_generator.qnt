// -*- mode: Bluespec; -*-

// =============================================================================
// Channel API Reference Generator (DRAFT)
// =============================================================================
//
// A minimal state machine that produces valid Channel API message sequences.
// Verified against channel_api_contract.qnt.
//
// This is NOT the full Malachite consensus spec. It models only the observable
// behavior at the Channel API boundary — just enough state to produce messages
// in the correct order with correct safety properties (agreement, validity).
//
// Consumers (e.g., Emerald) compose with this generator instead of writing
// ad-hoc listen_* functions that guess at Malachite's behavior.
//
// Design:
//   - Per-node state tracks the phase in the Channel API lifecycle
//   - Global state tracks decisions (for agreement) and proposals (for validity)
//   - Actions correspond to Channel API messages from Malachite to the app
//   - Ordering is enforced by the state machine structure (phase transitions)
//   - Safety is enforced by global state constraints
//   - All contract properties are checked via contractInv at every step
//   - Fault injection: nodeCrash (lose all state) and nodeRestart (preserve
//     disk state) clear the node's request history and reset to Unstarted
//
// State follows the disk/mem convention from faults.qnt:
//   - Disk: maxDecided (survives restart)
//   - Mem: phase, height, round, deliveredProposals, getValueSent (reset on fault)
//   - Request histories are mem state (cleared on fault, current session only)
//
// Phase transitions:
//   Unstarted ──sendConsensusReady──────────────> Started
//   Started   ──sendStartedRound────────────────> InRound
//   InRound   ──sendStartedRoundAfterTimeout─────> InRound  (new round, same height)
//   InRound   ──sendDecided─────────────────────> Started   (height h+1)
//   InRound   ──sendProcessSyncedValue──────────> Syncing
//   Syncing   ──sendProcessSyncedValue──────────> Syncing
//   Syncing   ──sendDecided─────────────────────> Started   (height h+1)
//   *         ──nodeCrash──────────────────────────> Unstarted  (all state lost)
//   *         ──nodeRestart────────────────────────> Unstarted  (disk preserved)

module channel_api_generator {
  import basicSpells.* from "spells/basicSpells"
  import rareSpells.* from "spells/rareSpells"
  import faults.* from "faults"
  import channel_api_contract.* from "channel_api_contract"

  // ===========================================================================
  // CONFIGURATION
  // ===========================================================================

  const NODES: List[Node]

  // Proposer selection — provided by the application (e.g., Emerald).
  // In the real system, the application defines this via the Malachite
  // Context trait. The generator (modeling Malachite) is parameterized
  // over it rather than defining it.
  const proposer_for: (Height, Round) => Node

  // Max height the generator will explore (bounds the trace length)
  pure val MAX_HEIGHT = 4

  // Max round per height (bounds timeout exploration)
  pure val MAX_ROUND = 3

  // ===========================================================================
  // PER-NODE STATE (disk/mem split)
  // ===========================================================================

  // Lifecycle phases for a node's view of the Channel API.
  // These model the sequence of messages a node receives from Malachite.
  //
  //   Unstarted
  //     → [ConsensusReady] →
  //   Started
  //     → [StartedRound(h, r)] →
  //   InRound
  //     → [GetValue(h, r)] if proposer →
  //   InRound (value proposed)
  //     → [Decided(h, v)] →
  //   Started (height h+1)
  //     → ...
  //
  //   InRound
  //     → [Timeout(h, r)] →
  //   Started (same height, round r+1)
  //
  type Phase =
    | Unstarted   // Before ConsensusReady
    | Started     // ConsensusReady received, ready for StartedRound
    | InRound     // StartedRound received, consensus in progress
    | Syncing     // Receiving synced values (catch-up)

  // Disk state: survives restart, lost on crash
  type GenDiskState = {
    // Latest height decided by this node. Since decisions are strictly
    // sequential, the max decided height fully characterizes all decided
    // heights. Prevents sendDecided and sendProcessSyncedValue from firing
    // twice for the same (node, height).
    maxDecided: Height,
  }

  // Mem state: cleared on any fault (crash or restart)
  type GenMemState = {
    phase: Phase,
    height: Height,
    round: Round,
    // Proposals Malachite has delivered to this node via GetValue (proposer),
    // ReceivedProposal (gossip), or ProcessSyncedValue (sync). Guards
    // sendDecided — a node can only decide on a proposal it has received.
    deliveredProposals: Set[Proposal],
    // Which (height, round) have had GetValue sent to this node.
    // Prevents duplicate GetValue for the same (h, r).
    getValueSent: Set[(Height, Round)],
  }

  type GenNodeState = {
    disk: GenDiskState,
    mem: GenMemState,
  }

  pure val initGenDisk: GenDiskState = {
    maxDecided: 0,
  }

  pure val initGenMem: GenMemState = {
    phase: Unstarted,
    height: 0,
    round: 0,
    deliveredProposals: Set(),
    getValueSent: Set(),
  }

  pure val initGenNode: GenNodeState = {
    disk: initGenDisk,
    mem: initGenMem,
  }

  // ===========================================================================
  // TRACE ENTRY (for lastEntry observation / MBT integration)
  // ===========================================================================
  //
  // Since faults are no longer part of ChannelRequest, we need a separate
  // type to represent what happened in the last step (channel message or fault).

  type TraceEntry =
    | ChannelMsg(ChannelRequest)
    | FaultMsg(FaultEvent)

  // ===========================================================================
  // GLOBAL STATE
  // ===========================================================================

  // Per-node Channel API state (disk/mem split per node)
  var nodeStates: Node -> GenNodeState

  // All proposals made via GetValue (for validity checking)
  var proposals: Set[Proposal]

  // Authoritative decisions: at most one per height.
  // Used as a guard in sendDecided (ensuring the same proposal is decided
  // across nodes) and by the contract's validity and syncedValueIsDecided
  // properties. The contract's agreement property checks request histories
  // directly, so it catches disagreement even if this map were buggy.
  var decisions: Height -> Proposal

  // Per-node request history for the current session (mem state).
  // Cleared on fault — contains only current-session requests.
  var requestHistory: Node -> List[ChannelRequest]

  // Per-proposal: which nodes have been delivered this specific proposal
  // and are in a position to vote on it. Keyed by Proposal (not Height)
  // because different rounds at the same height can have different
  // proposals from different proposers.
  // Disk state — a vote is cast when a proposal is received; this doesn't
  // un-happen on restart or crash (the network saw it).
  // Updated by:
  //   - sendGetValue: proposer participates at their height
  //   - sendReceivedProposal: at current height only (future-height
  //     proposals are pending — not voted on yet)
  //   - sendStartedRound: retroactive — if a node already has proposals
  //     for this height (received at a prior height as pending), count
  //     the node as support now that it has reached this height
  // NOT updated by sendProcessSyncedValue — already decided, quorum was met.
  // Guards sendDecided: requires > 2/3 support before any node can decide.
  var proposalSupport: Proposal -> Set[Node]

  // Last trace entry emitted (for trace observation / MBT integration).
  // None in the initial state (no entry yet).
  var lastEntry: Option[{ node: Node, entry: TraceEntry }]

  // ===========================================================================
  // ACTIONS: Channel API messages from Malachite to the application
  // ===========================================================================

  // Implementation details vs contract properties:
  //   Guards and behaviors in these actions fall into four categories:
  //
  //   1. CONTRACT ENFORCEMENT: directly enforces a named contract property
  //   2. IMPLEMENTATION DETAILS: internal state machine, not observable by contract
  //   3. OPTIMIZATIONS: search space bounds, not required by contract
  //   4. ASSUMPTIONS: application guarantee constraints on nondeterministic inputs
  //
  //   Inline comments below mark guards with [CONTRACT], [IMPL], [OPT], or
  //   [ASSUMPTION] to clarify their purpose.

  // --- ConsensusReady ---
  // Malachite has initialized and is ready. Sent once per node.
  // The application replies with a starting height (replyStartHeight).
  //
  // Source: AppMsg::ConsensusReady in msgs.rs
  action sendConsensusReady(node: Node, replyStartHeight: Height): bool = {
    val ns = nodeStates.get(node)
    all {
      // [CONTRACT] heightsPositive + consensusReadyResponse: all request heights >= 1
      replyStartHeight >= 1,
      // [IMPL] phase progression: must be Unstarted before ConsensusReady
      ns.mem.phase == Unstarted,
      val req = ReqConsensusReady({ respStartHeight: replyStartHeight })
      all {
        // state update
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: { ...ns.mem, phase: Started, height: replyStartHeight, round: 0 },
        }),
        proposals' = proposals,
        decisions' = decisions,
        proposalSupport' = proposalSupport,
        // bookkeeping
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
      }
    }
  }

  // --- StartedRound ---
  // The first round at a new height. Sent after ConsensusReady (height 1) or
  // after Decided (next height). Guard: phase == Started.
  // For timeout-driven round bumps at the same height, see sendStartedRoundAfterTimeout.
  //
  // Source: AppMsg::StartedRound in msgs.rs
  action sendStartedRound(node: Node): bool = {
    val ns = nodeStates.get(node)
    val h = ns.mem.height
    // [CONTRACT] quorumDecision: retroactive support — if the node already has
    // proposals for this height (received at a prior height as pending), count
    // it as support now that the node is at this height.
    val pendingAtHeight = ns.mem.deliveredProposals.filter(p => p.height == h)
    val updatedSupport = pendingAtHeight.fold(proposalSupport, (acc, p) =>
      val existing = if (acc.has(p)) acc.get(p) else Set()
      acc.put(p, existing.union(Set(node)))
    )
    all {
      // [IMPL] phase progression: must be Started (after ConsensusReady or Decided)
      ns.mem.phase == Started,
      // [OPT] search space bound: limits trace exploration
      ns.mem.height <= MAX_HEIGHT,
      val proposer = proposer_for(ns.mem.height, ns.mem.round)
      val req = ReqStartedRound({
        height: ns.mem.height,
        round: ns.mem.round,
        proposer: proposer,
      })
      all {
        // state update
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: { ...ns.mem, phase: InRound },
        }),
        proposals' = proposals,
        decisions' = decisions,
        proposalSupport' = updatedSupport,
        // bookkeeping
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
      }
    }
  }

  // --- GetValue ---
  // Request the application to build a value for consensus to propose.
  // Only sent to the proposer for (height, round). At most once per
  // (node, height, round). The application replies with a proposal
  // (replyProposal). The generator validates (height, round, proposer)
  // but the payload is the application's choice.
  //
  // Source: AppMsg::GetValue in msgs.rs
  action sendGetValue(node: Node, replyProposal: Proposal): bool = {
    val ns = nodeStates.get(node)
    all {
      // [CONTRACT] getValueUnique: at most once per (height, round)
      not(ns.mem.getValueSent.contains((ns.mem.height, ns.mem.round))),
      // [ASSUMPTION] application guarantee #3: reply matches current (height, round, proposer).
      // The payload is the application's choice — not constrained here.
      replyProposal.height == ns.mem.height,
      replyProposal.round == ns.mem.round,
      replyProposal.proposer == node,
      // [IMPL] phase progression: must be InRound (after StartedRound)
      ns.mem.phase == InRound,
      // [CONTRACT] getValueAfterStartedRound: only the proposer receives GetValue
      // Note: placed after [IMPL] phase check to avoid calling proposer_for at height 0
      node == proposer_for(ns.mem.height, ns.mem.round),
      val req = ReqGetValue({
        height: ns.mem.height,
        round: ns.mem.round,
      })
      // [CONTRACT] quorumDecision: proposer supports their own proposal
      val existing = if (proposalSupport.has(replyProposal)) proposalSupport.get(replyProposal) else Set()
      all {
        // state update
        proposals' = proposals.union(Set(replyProposal)),
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: {
            ...ns.mem,
            getValueSent: ns.mem.getValueSent.union(Set((ns.mem.height, ns.mem.round))),
            deliveredProposals: ns.mem.deliveredProposals.union(Set(replyProposal)),
          },
        }),
        decisions' = decisions,
        // Proposer supports their specific proposal
        proposalSupport' = proposalSupport.put(replyProposal, existing.union(Set(node))),
        // bookkeeping
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
      }
    }
  }

  // --- ReceivedProposal ---
  // Inform a non-proposer node about a proposal.
  // The proposal must exist (was created via GetValue).
  // Can arrive in any phase after ConsensusReady — including before
  // StartedRound (Emerald buffers as pending) or during sync.
  //
  // Source: AppMsg::ReceivedProposalPart in msgs.rs (simplified)
  action sendReceivedProposal(node: Node, proposal: Proposal): bool = {
    val ns = nodeStates.get(node)
    all {
      // [CONTRACT] receivedProposalExists: proposal was created via GetValue
      proposal.in(proposals),
      // [CONTRACT] receivedProposalNotToProposer: nodes don't receive their own proposals
      proposal.proposer != node,
      // [IMPL] phase progression: must have started (after ConsensusReady)
      ns.mem.phase != Unstarted,
      // [OPT] stale proposal filtering: real network can deliver stale proposals
      // (Emerald drops them). Not enforced by the contract.
      proposal.height >= ns.mem.height,
      val req = ReqReceivedProposal({ proposal: proposal })
      // [CONTRACT] quorumDecision: only count support when the node is at the
      // proposal's height. Future-height proposals are pending — not voted on yet.
      val atCurrentHeight = proposal.height == ns.mem.height
      val existing = if (proposalSupport.has(proposal)) proposalSupport.get(proposal) else Set()
      all {
        // state update
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: {
            ...ns.mem,
            deliveredProposals: ns.mem.deliveredProposals.union(Set(proposal)),
          },
        }),
        proposals' = proposals,
        decisions' = decisions,
        proposalSupport' = if (atCurrentHeight)
          proposalSupport.put(proposal, existing.union(Set(node)))
        else
          proposalSupport,
        // bookkeeping
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
      }
    }
  }

  // --- Decided ---
  // Consensus has decided on a value. The decided proposal must exist.
  //
  // AGREEMENT ENFORCEMENT: If a decision already exists for this height,
  // we must decide on the same proposal. If no decision exists yet, we
  // record this as the authoritative decision for this height.
  //
  // Source: AppMsg::Decided in msgs.rs
  // Safety: corresponds to AgreementInv in statemachineAsync.qnt
  action sendDecided(node: Node, proposal: Proposal): bool = {
    val ns = nodeStates.get(node)
    all {
      // [CONTRACT] validity: decided proposal must exist (was created via GetValue)
      proposal.in(proposals),
      // [CONTRACT] decidedRequiresDeliveredProposal: Malachite delivered the proposal
      ns.mem.deliveredProposals.contains(proposal),
      // [CONTRACT] agreement: if already decided for this height, must be the same
      not(decisions.has(ns.mem.height)) or decisions.get(ns.mem.height) == proposal,
      // [CONTRACT] quorumDecision: strictly more than 2/3 of nodes must have
      // received this specific proposal before any node can decide on it
      proposalSupport.has(proposal),
      proposalSupport.get(proposal).size() * 3 > NODES.length() * 2,
      // [IMPL] phase progression: must be InRound or Syncing
      or { ns.mem.phase == InRound, ns.mem.phase == Syncing },
      // [IMPL] height match: decision must be for the current height
      proposal.height == ns.mem.height,
      // [IMPL] prevent duplicate: not already decided at this node for this height
      ns.disk.maxDecided < ns.mem.height,
      // [OPT] future round filtering: restricts generator to avoid decisions
      // referencing a future round (contract's roundsConsecutiveWithinHeight
      // already constrains this in the normal path)
      proposal.round <= ns.mem.round,
      // state update
      // Record the decision
      decisions' = decisions.put(ns.mem.height, proposal),
      // Move to next height
      val req = ReqDecided({ proposal: proposal })
      all {
        // state update
        nodeStates' = nodeStates.set(node, {
          disk: { ...ns.disk, maxDecided: ns.mem.height },
          mem: { ...ns.mem, phase: Started, height: ns.mem.height + 1, round: 0 },
        }),
        proposals' = proposals,
        proposalSupport' = proposalSupport,
        // bookkeeping
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
      }
    }
  }

  // --- StartedRound after timeout ---
  // Consensus timed out waiting for progress in the current round.
  // Timeout is internal to Malachite — the application only sees the
  // resulting StartedRound at the next round.
  action sendStartedRoundAfterTimeout(node: Node): bool = {
    val ns = nodeStates.get(node)
    all {
      // [IMPL] phase progression: must be InRound (timeout during a round)
      ns.mem.phase == InRound,
      // [OPT] search space bound: limits round exploration
      ns.mem.round < MAX_ROUND,
      // [OPT] search space bound: limits height exploration
      ns.mem.height <= MAX_HEIGHT,
      val newRound = ns.mem.round + 1
      val proposer = proposer_for(ns.mem.height, newRound)
      val req = ReqStartedRound({
        height: ns.mem.height,
        round: newRound,
        proposer: proposer,
      })
      all {
        // state update
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: { ...ns.mem, phase: InRound, round: newRound },
        }),
        proposals' = proposals,
        decisions' = decisions,
        proposalSupport' = proposalSupport,
        // bookkeeping
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
      }
    }
  }

  // --- ProcessSyncedValue ---
  // A synced value arrives from the network (catch-up scenario).
  // The proposal must have been decided at some other node.
  //
  // Source: AppMsg::ProcessSyncedValue in msgs.rs
  action sendProcessSyncedValue(node: Node): bool = {
    val ns = nodeStates.get(node)
    all {
      // [CONTRACT] syncedValueIsDecided: synced value must have been decided
      decisions.has(ns.mem.height),
      // [IMPL] phase progression: must be InRound or Syncing
      or { ns.mem.phase == InRound, ns.mem.phase == Syncing },
      // [IMPL] prevent duplicate: not already decided at this node for this height
      ns.disk.maxDecided < ns.mem.height,
      val proposal = decisions.get(ns.mem.height)
      val req = ReqProcessSyncedValue({ proposal: proposal })
      all {
        // state update
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: {
            ...ns.mem,
            phase: Syncing,
            deliveredProposals: ns.mem.deliveredProposals.union(Set(proposal)),
          },
        }),
        proposals' = proposals,
        decisions' = decisions,
        // No support update — proposal is already decided, quorum was already met.
        proposalSupport' = proposalSupport,
        // bookkeeping
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
      }
    }
  }

  // --- GetDecidedValue ---
  // Malachite asks the application for a previously decided value (sync).
  //
  // Source: AppMsg::GetDecidedValue in msgs.rs
  action sendGetDecidedValue(node: Node, height: Height): bool = {
    val ns = nodeStates.get(node)
    all {
      // [OPT] heights start at 1: real Malachite can query any height
      height >= 1,
      // [OPT] only query past heights: restricts traces to reduce state space
      height < ns.mem.height,
      val req = ReqGetDecidedValue({ height: height })
      all {
        // state update
        nodeStates' = nodeStates,
        proposals' = proposals,
        decisions' = decisions,
        proposalSupport' = proposalSupport,
        // bookkeeping
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
      }
    }
  }

  // ===========================================================================
  // FAULT ACTIONS: crash and restart
  // ===========================================================================
  //
  // Faults clear the node's request history (mem state) instead of appending
  // ReqFault. This keeps histories short (bounded by session length) and
  // eliminates session-boundary logic from all contract properties.

  // --- Node Crash ---
  // Node lost all state: reset disk + mem, clear request history.
  action nodeCrash(node: Node): bool = {
    val ns = nodeStates.get(node)
    all {
      // [IMPL] must be running: can't crash a node that hasn't started
      ns.mem.phase != Unstarted,
      // state update
      nodeStates' = nodeStates.set(node, initGenNode),
      proposals' = proposals,
      decisions' = decisions,
      // Disk state — votes already cast don't un-happen
      proposalSupport' = proposalSupport,
      // bookkeeping
      requestHistory' = requestHistory.set(node, []),
      lastEntry' = Some({ node: node, entry: FaultMsg(Crash) }),
    }
  }

  // --- Node Restart ---
  // Node restarted: preserve disk state, reset mem, clear request history.
  // Decisions survive on disk (maxDecided preserved). Malachite re-initializes
  // and will re-deliver proposals and ConsensusReady.
  action nodeRestart(node: Node): bool = {
    val ns = nodeStates.get(node)
    all {
      // [IMPL] must be running: can't restart a node that hasn't started
      ns.mem.phase != Unstarted,
      // state update
      nodeStates' = nodeStates.set(node, { disk: ns.disk, mem: initGenMem }),
      proposals' = proposals,
      decisions' = decisions,
      // Disk state — votes already cast don't un-happen
      proposalSupport' = proposalSupport,
      // bookkeeping
      requestHistory' = requestHistory.set(node, []),
      lastEntry' = Some({ node: node, entry: FaultMsg(Restart) }),
    }
  }

  // ===========================================================================
  // PASSTHROUGH ACTION (for composition)
  // ===========================================================================

  // Stutter action: no Channel API activity this step.
  // Exported for composition steps that only interact with Engine API.
  action genUnchanged: bool = all {
    nodeStates' = nodeStates,
    proposals' = proposals,
    decisions' = decisions,
    requestHistory' = requestHistory,
    proposalSupport' = proposalSupport,
    lastEntry' = lastEntry,
  }

  // ===========================================================================
  // INITIALIZATION
  // ===========================================================================

  action init = {
    val nodes = NODES.toSet()
    all {
      nodeStates' = nodes.mapBy(n => initGenNode),
      proposals' = Set(),
      decisions' = Map(),
      requestHistory' = nodes.mapBy(n => []),
      proposalSupport' = Map(),
      lastEntry' = None,
    }
  }

  // ===========================================================================
  // STEP: nondeterministically pick a node and a valid action
  // ===========================================================================

  action step = {
    val nodes = NODES.toSet()
    nondet node = nodes.oneOf()
    any {
      sendConsensusReady(node, 1),
      sendStartedRound(node),
      val ns = nodeStates.get(node)
      sendGetValue(node, {
        height: ns.mem.height,
        round: ns.mem.round,
        proposer: node,
        payload: proposals.size(),
      }),
      nondet proposal = proposals.oneOf()
      sendReceivedProposal(node, proposal),
      nondet proposal = proposals.oneOf()
      sendDecided(node, proposal),
      sendProcessSyncedValue(node),
      nondet height = 1.to(MAX_HEIGHT).oneOf()
      sendGetDecidedValue(node, height),
      nodeCrash(node),
      nodeRestart(node),
    }
  }

  // ===========================================================================
  // CONTRACT PROPERTIES (checked as invariants)
  // ===========================================================================

  // Build the observable ChannelState for contract checking.
  // Only the fields used by contract properties are included.
  // Internal generator state (nodeStates) is not exposed — it exists
  // only for the generator's own guards.
  val channelState: ChannelState = {
    disk: {
      all_proposals: proposals,
      decisions: decisions,
      proposal_support: proposalSupport,
    },
    mem: {
      request_history: requestHistory,
    },
  }

  // Safety invariants from the contract
  val agreementInv = agreement(channelState)
  val validityInv = validity(channelState)
  val safetyInv = safety(channelState, NODES.length())

  // Ordering invariants from the contract
  val orderingInv = ordering(channelState)

  // Server (Emerald) guarantees from the contract
  val applicationGuaranteesInv = applicationGuarantees(channelState)

  // Client (Malachite) guarantees: safety + ordering
  val contractInv = componentGuarantees(channelState, NODES.length())

}
