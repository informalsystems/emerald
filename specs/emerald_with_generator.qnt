// -*- mode: Bluespec; -*-

// =============================================================================
// Emerald + Channel API Generator Composition
// =============================================================================
//
// Composes the Channel API reference generator with Emerald's handler logic.
// Replaces the Choreo-based listen_* functions and Extensions oracle from
// emerald.qnt with the generator's state machine.
//
// Architecture:
//   Generator picks node + action -> Generator updates its own state
//                                 -> Emerald handler updates emeraldState
//
// Each step is: all { gen::sendX(node), handleX(node) }
// Generator and Emerald actions fire atomically, each assigning their own
// state variables.
//
// State follows the disk/mem convention from faults.qnt:
//   - Disk: proposals, last_decided_height, last_decided_payload (survive restart)
//   - Mem: phase, consensus_height, consensus_round (reset on fault)
//
// Scope: Normal lifecycle + timeouts + sync + crash/restart.

module emerald_with_generator {
  import basicSpells.* from "spells/basicSpells"
  import rareSpells.* from "spells/rareSpells"
  import channel_api_contract.* from "channel_api_contract"

  const NODES: List[Node]

  // Proposer selection — application-defined, passed to the generator.
  // Rotating proposer based on (height, round).
  pure def proposer_for(height: Height, round: Round): Node =
    NODES[(height - 1 + round) % NODES.length()]

  import channel_api_generator(NODES = NODES, proposer_for = proposer_for) as gen from "channel_api_generator"

  // ===========================================================================
  // EMERALD-SPECIFIC TYPES (disk/mem split)
  // ===========================================================================

  type AppPhase =
    | Uninitialized  // Before ConsensusReady
    | Ready          // After ConsensusReady or Decided, before StartedRound
    | Working        // After StartedRound
    | Syncing        // After ProcessSyncedValue

  // Disk state: survives restart, lost on crash
  type EmeraldDiskState = {
    proposals: Set[Proposal],
    last_decided_height: Height,
    last_decided_payload: Option[Payload],
  }

  // Mem state: cleared on any fault (crash or restart)
  type EmeraldMemState = {
    phase: AppPhase,
    consensus_height: Height,
    consensus_round: Round,
  }

  type EmeraldNodeState = {
    disk: EmeraldDiskState,
    mem: EmeraldMemState,
  }

  // ===========================================================================
  // STATE
  // ===========================================================================

  var emeraldState: Node -> EmeraldNodeState

  // ===========================================================================
  // INITIALIZATION
  // ===========================================================================

  pure val initEmeraldDisk: EmeraldDiskState = {
    proposals: Set(),
    last_decided_height: 0,
    last_decided_payload: None,
  }

  pure val initEmeraldMem: EmeraldMemState = {
    phase: Uninitialized,
    consensus_height: 0,
    consensus_round: 0,
  }

  pure val initEmeraldNode: EmeraldNodeState = {
    disk: initEmeraldDisk,
    mem: initEmeraldMem,
  }

  action init = all {
    gen::init,
    emeraldState' = NODES.toSet().mapBy(n => initEmeraldNode),
  }

  // ===========================================================================
  // HANDLERS: Emerald's response to each Channel API message
  // ===========================================================================

  // ConsensusReady: Initialize node. The starting height is passed in
  // (computed from the node's last decided height in the composed step).
  action handleConsensusReady(node: Node, startHeight: Height): bool = {
    val s = emeraldState.get(node)
    all {
      s.mem.phase == Uninitialized,
      emeraldState' = emeraldState.set(node, {
        ...s,
        mem: { ...s.mem, phase: Ready, consensus_height: startHeight, consensus_round: 0 },
      }),
    }
  }

  // StartedRound: Enter new consensus round.
  // Height and round come from the StartedRound message payload.
  action handleStartedRound(node: Node, height: Height, round: Round): bool =
    emeraldState' = emeraldState.set(node, {
      ...emeraldState.get(node),
      mem: { phase: Working, consensus_height: height, consensus_round: round },
    })

  // GetValue: Proposer builds or returns a value.
  // The proposal is computed in the composed step and passed to both the
  // generator (which records it) and this handler (which stores it locally).
  action handleGetValue(node: Node, proposal: Proposal): bool = {
    val s = emeraldState.get(node)
    emeraldState' = emeraldState.set(node, {
      ...s,
      disk: { ...s.disk, proposals: s.disk.proposals.union(Set(proposal)) },
    })
  }

  // ReceivedProposal: Non-proposer learns about a proposal
  action handleReceivedProposal(node: Node, proposal: Proposal): bool = {
    val s = emeraldState.get(node)
    emeraldState' = emeraldState.set(node, {
      ...s,
      disk: { ...s.disk, proposals: s.disk.proposals.union(Set(proposal)) },
    })
  }

  // Decided: Consensus decided on a value, advance to next height.
  // TODO: Model the proposal lifecycle (pending -> undecided -> decided).
  // In the implementation, Decided carries only a certificate — the node
  // must already have the proposal stored as undecided. The generator's
  // decidedRequiresDeliveredProposal contract property enforces this
  // ordering from Malachite's perspective, but Emerald should track
  // pending/undecided proposals in its own state to match the
  // implementation more closely.
  action handleDecided(node: Node, proposal: Proposal): bool = {
    val s = emeraldState.get(node)
    emeraldState' = emeraldState.set(node, {
      disk: {
        ...s.disk,
        last_decided_height: proposal.height,
        last_decided_payload: Some(proposal.payload),
        proposals: s.disk.proposals.union(Set(proposal)),
      },
      mem: {
        phase: Ready,
        consensus_height: proposal.height + 1,
        consensus_round: 0,
      },
    })
  }

  // ProcessSyncedValue: Catch-up with a synced decided value.
  // The proposal comes from the ProcessSyncedValue message payload.
  action handleProcessSyncedValue(node: Node, proposal: Proposal): bool = {
    val s = emeraldState.get(node)
    emeraldState' = emeraldState.set(node, {
      ...s,
      mem: { ...s.mem, phase: Syncing },
      disk: { ...s.disk, proposals: s.disk.proposals.union(Set(proposal)) },
    })
  }

  // GetDecidedValue: Query for past decided value (no Emerald state change)
  action handleGetDecidedValue(node: Node, height: Height): bool =
    emeraldState' = emeraldState

  // NodeCrash: Full reset — all state lost (disk + mem).
  action handleNodeCrash(node: Node): bool =
    emeraldState' = emeraldState.set(node, initEmeraldNode)

  // NodeRestart: Preserve disk state, reset mem.
  // Proposals + last_decided_* survive on disk. Phase and consensus
  // height/round reset — Malachite will re-send ConsensusReady.
  action handleNodeRestart(node: Node): bool = {
    val s = emeraldState.get(node)
    emeraldState' = emeraldState.set(node, { disk: s.disk, mem: initEmeraldMem })
  }

  // ===========================================================================
  // PROPOSAL CONSTRUCTION
  // ===========================================================================

  // Build a proposal for GetValue: return existing proposal for (h, r) or create new.
  pure def buildProposal(s: EmeraldNodeState, node: Node, numProposals: int): Proposal = {
    val existing = s.disk.proposals.find(p =>
      p.height == s.mem.consensus_height and p.round == s.mem.consensus_round
    )
    match existing {
      | Some(p) => p
      | None => {
          height: s.mem.consensus_height,
          round: s.mem.consensus_round,
          proposer: node,
          payload: numProposals,
        }
    }
  }

  // ===========================================================================
  // COMPOSED STEPS
  // ===========================================================================

  action stepConsensusReady(node: Node): bool = {
    val startHeight = emeraldState.get(node).disk.last_decided_height + 1
    all { gen::sendConsensusReady(node, startHeight), handleConsensusReady(node, startHeight) }
  }

  action stepStartedRound(node: Node): bool = {
    val ns = gen::nodeStates.get(node).mem
    any {
      // Normal: after ConsensusReady or Decided
      all {
        gen::sendStartedRound(node),
        handleStartedRound(node, ns.height, ns.round),
      },
      // After timeout: Malachite internally timed out, app sees StartedRound at next round
      all {
        gen::sendStartedRoundAfterTimeout(node),
        handleStartedRound(node, ns.height, ns.round + 1),
      },
    }
  }

  action stepGetValue(node: Node): bool = {
    val proposal = buildProposal(emeraldState.get(node), node, gen::proposals.size())
    all { gen::sendGetValue(node, proposal), handleGetValue(node, proposal) }
  }

  action stepReceivedProposal(node: Node, proposal: Proposal): bool =
    all { gen::sendReceivedProposal(node, proposal), handleReceivedProposal(node, proposal) }

  action stepDecided(node: Node, proposal: Proposal): bool =
    all { gen::sendDecided(node, proposal), handleDecided(node, proposal) }

  action stepProcessSyncedValue(node: Node): bool = {
    val ns = gen::nodeStates.get(node).mem
    val proposal = gen::decisions.get(ns.height)
    all { gen::sendProcessSyncedValue(node), handleProcessSyncedValue(node, proposal) }
  }

  action stepGetDecidedValue(node: Node, height: Height): bool =
    all { gen::sendGetDecidedValue(node, height), handleGetDecidedValue(node, height) }

  action stepNodeCrash(node: Node): bool =
    all { gen::nodeCrash(node), handleNodeCrash(node) }

  action stepNodeRestart(node: Node): bool =
    all { gen::nodeRestart(node), handleNodeRestart(node) }

  action step = {
    nondet node = NODES.toSet().oneOf()
    any {
      stepConsensusReady(node),
      stepStartedRound(node),
      stepGetValue(node),
      nondet proposal = gen::proposals.oneOf()
      stepReceivedProposal(node, proposal),
      nondet proposal = gen::proposals.oneOf()
      stepDecided(node, proposal),
      stepProcessSyncedValue(node),
      nondet height = 1.to(gen::MAX_HEIGHT).oneOf()
      stepGetDecidedValue(node, height),
      stepNodeCrash(node),
      stepNodeRestart(node),
    }
  }

  // ===========================================================================
  // INVARIANTS
  // ===========================================================================

  // Safety: All nodes agree on decided blocks.
  // This is backed by the generator's contract guarantee (agreement), not
  // by a hand-crafted oracle.
  val emerald_agreement =
    NODES.toSet().forall(n1 =>
      NODES.toSet().forall(n2 =>
        val st1 = emeraldState.get(n1)
        val st2 = emeraldState.get(n2)
        st1.disk.last_decided_height == st2.disk.last_decided_height implies
          st1.disk.last_decided_payload == st2.disk.last_decided_payload
      )
    )

  // Consistency: After initialization, consensus height is always greater
  // than the last decided height.
  val consensus_height_gt_last_decided_height =
    NODES.toSet().forall(n =>
      val st = emeraldState.get(n)
      st.mem.consensus_height > 0 implies
        st.mem.consensus_height > st.disk.last_decided_height
    )

  // Completeness: All nodes know all decided proposals up to their last
  // decided height.
  val completion =
    NODES.toSet().forall(n =>
      val st = emeraldState.get(n)
      1.to(st.disk.last_decided_height).forall(height =>
        gen::decisions.has(height) and gen::decisions.get(height).in(st.disk.proposals)
      )
    )

  // Contract: The generator's Channel API contract is verified separately
  // in channel_api_generator_test.qnt. Checking it here adds ~11 history
  // scans per step for properties already enforced by the generator's
  // guards. Uncomment for full verification runs.
  // val contractInv = gen::contractInv
}
