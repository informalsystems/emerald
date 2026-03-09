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

  // --- ConsensusReady ---
  // Malachite has initialized and is ready. Sent once per node.
  // The application replies with a starting height (replyStartHeight).
  //
  // Source: AppMsg::ConsensusReady in msgs.rs
  action sendConsensusReady(node: Node, replyStartHeight: Height): bool = {
    val ns = nodeStates.get(node)
    all {
      ns.mem.phase == Unstarted,
      // Contract: heightsPositive requires all request heights >= 1.
      // This guard ensures the starting height satisfies that property.
      replyStartHeight >= 1,
      val req = ReqConsensusReady({ respStartHeight: replyStartHeight })
      all {
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: { ...ns.mem, phase: Started, height: replyStartHeight, round: 0 },
        }),
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
        proposals' = proposals,
        decisions' = decisions,
        proposalSupport' = proposalSupport,
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
    // If the node already has proposals for this height (received at a
    // prior height and buffered as pending), count it as support for
    // each such proposal now that the node is at this height.
    val pendingAtHeight = ns.mem.deliveredProposals.filter(p => p.height == h)
    val updatedSupport = pendingAtHeight.fold(proposalSupport, (acc, p) =>
      val existing = if (acc.has(p)) acc.get(p) else Set()
      acc.put(p, existing.union(Set(node)))
    )
    all {
      ns.mem.phase == Started,
      ns.mem.height <= MAX_HEIGHT,
      val proposer = proposer_for(ns.mem.height, ns.mem.round)
      val req = ReqStartedRound({
        height: ns.mem.height,
        round: ns.mem.round,
        proposer: proposer,
      })
      all {
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: { ...ns.mem, phase: InRound },
        }),
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
        proposals' = proposals,
        decisions' = decisions,
        proposalSupport' = updatedSupport,
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
      ns.mem.phase == InRound,
      node == proposer_for(ns.mem.height, ns.mem.round),
      not(ns.mem.getValueSent.contains((ns.mem.height, ns.mem.round))),
      // Assumption (not a Malachite guarantee): the application's reply matches
      // the current (height, round, proposer). Documented as assumption #3 in the
      // contract. The payload is the application's choice — not constrained here.
      replyProposal.height == ns.mem.height,
      replyProposal.round == ns.mem.round,
      replyProposal.proposer == node,
      val req = ReqGetValue({
        height: ns.mem.height,
        round: ns.mem.round,
      })
      val existing = if (proposalSupport.has(replyProposal)) proposalSupport.get(replyProposal) else Set()
      all {
        proposals' = proposals.union(Set(replyProposal)),
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: {
            ...ns.mem,
            getValueSent: ns.mem.getValueSent.union(Set((ns.mem.height, ns.mem.round))),
            deliveredProposals: ns.mem.deliveredProposals.union(Set(replyProposal)),
          },
        }),
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
        decisions' = decisions,
        // Proposer supports their specific proposal
        proposalSupport' = proposalSupport.put(replyProposal, existing.union(Set(node))),
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
      ns.mem.phase != Unstarted,
      proposal.in(proposals),
      proposal.proposer != node,  // Nodes don't receive their own proposals
      // Optimization: real network can deliver stale proposals (Emerald drops them).
      // Not enforced by the contract — restricts traces to reduce state space.
      proposal.height >= ns.mem.height,
      val req = ReqReceivedProposal({ proposal: proposal })
      // Only count support when the node is at the proposal's height.
      // Future-height proposals are buffered as pending — the node hasn't
      // participated in consensus at that height yet.
      val atCurrentHeight = proposal.height == ns.mem.height
      val existing = if (proposalSupport.has(proposal)) proposalSupport.get(proposal) else Set()
      all {
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: {
            ...ns.mem,
            deliveredProposals: ns.mem.deliveredProposals.union(Set(proposal)),
          },
        }),
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
        proposals' = proposals,
        decisions' = decisions,
        proposalSupport' = if (atCurrentHeight)
          proposalSupport.put(proposal, existing.union(Set(node)))
        else
          proposalSupport,
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
      or { ns.mem.phase == InRound, ns.mem.phase == Syncing },
      proposal.height == ns.mem.height,
      proposal.in(proposals),
      // Malachite must have delivered the proposal to this node
      ns.mem.deliveredProposals.contains(proposal),
      // Optimization: in the normal (non-sync) path, the contract's
      // decidedRequiresDeliveredProposal + roundsConsecutiveWithinHeight
      // already constrain this. This guard further restricts the generator
      // to avoid producing traces where a decision references a future round.
      proposal.round <= ns.mem.round,
      // Agreement: if already decided for this height, must be the same
      not(decisions.has(ns.mem.height)) or decisions.get(ns.mem.height) == proposal,
      // Not already decided at this node for this height
      ns.disk.maxDecided < ns.mem.height,
      // Quorum: strictly more than 2/3 of nodes must have received
      // this specific proposal before any node can decide on it.
      // Assumes equal voting power — uses node count as threshold.
      proposalSupport.has(proposal),
      proposalSupport.get(proposal).size() * 3 > NODES.length() * 2,
      // Record the decision
      decisions' = decisions.put(ns.mem.height, proposal),
      // Move to next height
      val req = ReqDecided({ proposal: proposal })
      all {
        nodeStates' = nodeStates.set(node, {
          disk: { ...ns.disk, maxDecided: ns.mem.height },
          mem: { ...ns.mem, phase: Started, height: ns.mem.height + 1, round: 0 },
        }),
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
        proposals' = proposals,
        proposalSupport' = proposalSupport,
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
      ns.mem.phase == InRound,
      ns.mem.round < MAX_ROUND,
      ns.mem.height <= MAX_HEIGHT,
      val newRound = ns.mem.round + 1
      val proposer = proposer_for(ns.mem.height, newRound)
      val req = ReqStartedRound({
        height: ns.mem.height,
        round: newRound,
        proposer: proposer,
      })
      all {
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: { ...ns.mem, phase: InRound, round: newRound },
        }),
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
        proposals' = proposals,
        decisions' = decisions,
        proposalSupport' = proposalSupport,
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
      or { ns.mem.phase == InRound, ns.mem.phase == Syncing },
      decisions.has(ns.mem.height),
      ns.disk.maxDecided < ns.mem.height,
      val proposal = decisions.get(ns.mem.height)
      val req = ReqProcessSyncedValue({ proposal: proposal })
      all {
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: {
            ...ns.mem,
            phase: Syncing,
            deliveredProposals: ns.mem.deliveredProposals.union(Set(proposal)),
          },
        }),
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
        proposals' = proposals,
        decisions' = decisions,
        // No support update — proposal is already decided, quorum was already met.
        proposalSupport' = proposalSupport,
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
      // Optimization: real Malachite can query any height — the app returns
      // None for unavailable heights. These guards restrict traces to reduce
      // state space; not enforced by the contract.
      height >= 1,
      height < ns.mem.height,
      val req = ReqGetDecidedValue({ height: height })
      all {
        lastEntry' = Some({ node: node, entry: ChannelMsg(req) }),
        requestHistory' = requestHistory.set(node,
          requestHistory.get(node).append(req)
        ),
        nodeStates' = nodeStates,
        proposals' = proposals,
        decisions' = decisions,
        proposalSupport' = proposalSupport,
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
      ns.mem.phase != Unstarted,
      nodeStates' = nodeStates.set(node, initGenNode),
      requestHistory' = requestHistory.set(node, []),
      lastEntry' = Some({ node: node, entry: FaultMsg(Crash) }),
      proposals' = proposals,
      decisions' = decisions,
      // Disk state — votes already cast don't un-happen
      proposalSupport' = proposalSupport,
    }
  }

  // --- Node Restart ---
  // Node restarted: preserve disk state, reset mem, clear request history.
  // Decisions survive on disk (maxDecided preserved). Malachite re-initializes
  // and will re-deliver proposals and ConsensusReady.
  action nodeRestart(node: Node): bool = {
    val ns = nodeStates.get(node)
    all {
      ns.mem.phase != Unstarted,
      nodeStates' = nodeStates.set(node, { disk: ns.disk, mem: initGenMem }),
      requestHistory' = requestHistory.set(node, []),
      lastEntry' = Some({ node: node, entry: FaultMsg(Restart) }),
      proposals' = proposals,
      decisions' = decisions,
      // Disk state — votes already cast don't un-happen
      proposalSupport' = proposalSupport,
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

  // Application safety invariants from the contract
  val applicationSafetyInv = applicationSafety(channelState)

  // Full contract (safety + ordering)
  val contractInv = contract(channelState, NODES.length())

}
