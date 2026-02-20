// -*- mode: Bluespec; -*-

// =============================================================================
// Channel API Reference Generator (DRAFT)
// =============================================================================
//
// A minimal state machine that produces valid Channel API message sequences.
// Written by the Malachite team. Verified against channel_api_contract.qnt.
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
//   - All 11 contract properties (2 safety + 9 ordering) are checked via
//     contractInv at every step
//   - Fault injection: nodeCrash (lose all state) and nodeRestart (preserve
//     disk state) clear the node's event history and reset to Unstarted
//
// State follows the disk/mem convention from faults.qnt:
//   - Disk: maxDecided (survives restart)
//   - Mem: phase, height, round, deliveredProposals, getValueSent (reset on fault)
//   - Event histories are mem state (cleared on fault, current session only)

module channel_api_generator {
  import basicSpells.* from "spells/basicSpells"
  import rareSpells.* from "spells/rareSpells"
  import faults.* from "faults"
  import channel_api_contract.* from "channel_api_contract"

  // ===========================================================================
  // CONFIGURATION
  // ===========================================================================

  const NODES: List[Node]

  // Proposer selection (matches Emerald's rotating proposer)
  pure def proposer_for(height: Height, round: Round): Node =
    NODES[(height - 1 + round) % NODES.length()]

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
  // TRACE EVENT (for lastEvent observation / MBT integration)
  // ===========================================================================
  //
  // Since faults are no longer part of ChannelEvent, we need a separate
  // type to represent what happened in the last step (channel message or fault).

  type TraceEvent =
    | ChannelMsg(ChannelEvent)
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
  // properties. The contract's agreement property checks event histories
  // directly, so it catches disagreement even if this map were buggy.
  var decisions: Height -> Proposal

  // Per-node event history for the current session (mem state).
  // Cleared on fault — contains only current-session events.
  var eventHistory: Node -> List[ChannelEvent]

  // Last event emitted (for trace observation / MBT integration).
  // None in the initial state (no event sent yet).
  var lastEvent: Option[{ node: Node, event: TraceEvent }]

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
      replyStartHeight >= 1,
      val evt = EvConsensusReady
      all {
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: { ...ns.mem, phase: Started, height: replyStartHeight, round: 0 },
        }),
        lastEvent' = Some({ node: node, event: ChannelMsg(evt) }),
        eventHistory' = eventHistory.set(node,
          eventHistory.get(node).append(evt)
        ),
        proposals' = proposals,
        decisions' = decisions,
      }
    }
  }

  // --- StartedRound ---
  // A new consensus round has begun. Sent after ConsensusReady (for the first
  // round), after Decided (for the first round of a new height), or after a
  // timeout (for subsequent rounds at the same height).
  //
  // Source: AppMsg::StartedRound in msgs.rs
  action sendStartedRound(node: Node): bool = {
    val ns = nodeStates.get(node)
    all {
      ns.mem.phase == Started,
      ns.mem.height <= MAX_HEIGHT,
      val proposer = proposer_for(ns.mem.height, ns.mem.round)
      val evt = EvStartedRound({
        height: ns.mem.height,
        round: ns.mem.round,
        proposer: proposer,
      })
      all {
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: { ...ns.mem, phase: InRound },
        }),
        lastEvent' = Some({ node: node, event: ChannelMsg(evt) }),
        eventHistory' = eventHistory.set(node,
          eventHistory.get(node).append(evt)
        ),
        proposals' = proposals,
        decisions' = decisions,
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
      // Validate the reply proposal matches the current (height, round, proposer).
      // The payload is the application's choice — not constrained here.
      replyProposal.height == ns.mem.height,
      replyProposal.round == ns.mem.round,
      replyProposal.proposer == node,
      val evt = EvGetValue({
        height: ns.mem.height,
        round: ns.mem.round,
      })
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
        lastEvent' = Some({ node: node, event: ChannelMsg(evt) }),
        eventHistory' = eventHistory.set(node,
          eventHistory.get(node).append(evt)
        ),
        decisions' = decisions,
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
      proposal.height >= ns.mem.height,  // Can arrive for future heights (buffered as pending)
      val evt = EvReceivedProposal({ proposal: proposal })
      all {
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: {
            ...ns.mem,
            deliveredProposals: ns.mem.deliveredProposals.union(Set(proposal)),
          },
        }),
        lastEvent' = Some({ node: node, event: ChannelMsg(evt) }),
        eventHistory' = eventHistory.set(node,
          eventHistory.get(node).append(evt)
        ),
        proposals' = proposals,
        decisions' = decisions,
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
      // Node must have reached the proposal's round (no round skips:
      // if ns.round >= p.round, all rounds up to ns.round were entered)
      proposal.round <= ns.mem.round,
      // Agreement: if already decided for this height, must be the same
      not(decisions.has(ns.mem.height)) or decisions.get(ns.mem.height) == proposal,
      // Not already decided at this node for this height
      ns.disk.maxDecided < ns.mem.height,
      // Record the decision
      decisions' = decisions.put(ns.mem.height, proposal),
      // Move to next height
      val evt = EvDecided({ proposal: proposal })
      all {
        nodeStates' = nodeStates.set(node, {
          disk: { ...ns.disk, maxDecided: ns.mem.height },
          mem: { ...ns.mem, phase: Started, height: ns.mem.height + 1, round: 0 },
        }),
        lastEvent' = Some({ node: node, event: ChannelMsg(evt) }),
        eventHistory' = eventHistory.set(node,
          eventHistory.get(node).append(evt)
        ),
        proposals' = proposals,
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
      val evt = EvStartedRound({
        height: ns.mem.height,
        round: newRound,
        proposer: proposer,
      })
      all {
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: { ...ns.mem, phase: InRound, round: newRound },
        }),
        lastEvent' = Some({ node: node, event: ChannelMsg(evt) }),
        eventHistory' = eventHistory.set(node,
          eventHistory.get(node).append(evt)
        ),
        proposals' = proposals,
        decisions' = decisions,
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
      val evt = EvProcessSyncedValue({ proposal: proposal })
      all {
        nodeStates' = nodeStates.set(node, {
          ...ns,
          mem: {
            ...ns.mem,
            phase: Syncing,
            deliveredProposals: ns.mem.deliveredProposals.union(Set(proposal)),
          },
        }),
        lastEvent' = Some({ node: node, event: ChannelMsg(evt) }),
        eventHistory' = eventHistory.set(node,
          eventHistory.get(node).append(evt)
        ),
        proposals' = proposals,
        decisions' = decisions,
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
      height >= 1,
      height < ns.mem.height,
      val evt = EvGetDecidedValue({ height: height })
      all {
        lastEvent' = Some({ node: node, event: ChannelMsg(evt) }),
        eventHistory' = eventHistory.set(node,
          eventHistory.get(node).append(evt)
        ),
        nodeStates' = nodeStates,
        proposals' = proposals,
        decisions' = decisions,
      }
    }
  }

  // ===========================================================================
  // FAULT ACTIONS: crash and restart
  // ===========================================================================
  //
  // Faults clear the node's event history (mem state) instead of appending
  // EvFault. This keeps histories short (bounded by session length) and
  // eliminates session-boundary logic from all contract properties.

  // --- Node Crash ---
  // Node lost all state: reset disk + mem, clear event history.
  action nodeCrash(node: Node): bool = {
    val ns = nodeStates.get(node)
    all {
      ns.mem.phase != Unstarted,
      nodeStates' = nodeStates.set(node, initGenNode),
      eventHistory' = eventHistory.set(node, []),
      lastEvent' = Some({ node: node, event: FaultMsg(Crash) }),
      proposals' = proposals,
      decisions' = decisions,
    }
  }

  // --- Node Restart ---
  // Node restarted: preserve disk state, reset mem, clear event history.
  // Decisions survive on disk (maxDecided preserved). Malachite re-initializes
  // and will re-deliver proposals and ConsensusReady.
  action nodeRestart(node: Node): bool = {
    val ns = nodeStates.get(node)
    all {
      ns.mem.phase != Unstarted,
      nodeStates' = nodeStates.set(node, { disk: ns.disk, mem: initGenMem }),
      eventHistory' = eventHistory.set(node, []),
      lastEvent' = Some({ node: node, event: FaultMsg(Restart) }),
      proposals' = proposals,
      decisions' = decisions,
    }
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
      eventHistory' = nodes.mapBy(n => []),
      lastEvent' = None,
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
    },
    mem: {
      event_history: eventHistory,
    },
  }

  // Safety invariants from the contract
  val agreementInv = agreement(channelState)
  val validityInv = validity(channelState)
  val safetyInv = safety(channelState)

  // Ordering invariants from the contract
  val orderingInv = ordering(channelState)

  // Full contract (safety + ordering)
  val contractInv = contract(channelState)

  // ===========================================================================
  // GENERATOR-SPECIFIC INVARIANTS (ordering, enforced by state machine)
  // ===========================================================================

  // Height is always positive for started nodes
  val heightPositive = NODES.toSet().forall(n =>
    nodeStates.get(n).mem.phase != Unstarted implies nodeStates.get(n).mem.height > 0
  )

  // Decided heights are contiguous (no gaps)
  val decisionsContiguous = decisions.keys().forall(h =>
    h == 1 or decisions.has(h - 1)
  )
}
