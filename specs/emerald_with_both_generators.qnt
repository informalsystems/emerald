// -*- mode: Bluespec; -*-

// =============================================================================
// Emerald + Channel API Generator + Engine API Generator Composition
// =============================================================================
//
// Three-way composition connecting both boundaries:
//   Channel API Generator → Emerald handlers → Engine API Generator
//
// Architecture:
//   - Channel API generator models Malachite sending requests to Emerald
//   - Emerald handlers process requests and make Engine API calls
//   - Engine API generator models Reth responding to Emerald's calls
//
// Mapping from Channel API requests to Engine API requests (from Emerald's app.rs):
//   | Channel API Request   | Engine API Requests                           |
//   |-----------------------|-----------------------------------------------|
//   | ConsensusReady        | (none — Phase 2 adds exchangeCapabilities)    |
//   | StartedRound          | none                                          |
//   | GetValue              | forkchoiceUpdated(head, attrs) → getPayload   |
//   | ReceivedProposal      | newPayload(block)                             |
//   | Decided               | newPayload(block) → forkchoiceUpdated(hash)   |
//   | ProcessSyncedValue    | none (validation deferred to Decided)         |
//
// Non-atomic Engine API composition:
//   Channel API events are received in one transition, then Engine API calls
//   are executed one-per-transition via a queue + continuation pattern.
//   This exposes intermediate states (e.g., mid-processing crashes) that
//   the atomic model would hide. Interleaving model: fault-only for the
//   same node (no new Channel events while processing), full interleaving
//   for other nodes.
//
// State follows the disk/mem convention from faults.qnt.
// Extends emerald_with_generator.qnt with Engine API tracking state.

module emerald_with_both_generators {
  import basicSpells.* from "spells/basicSpells"
  import rareSpells.* from "spells/rareSpells"
  import channel_api_contract.* from "channel_api_contract"
  import engine_api_contract as eac from "engine_api_contract"

  const NODES: List[Node]

  // Proposer selection — application-defined, passed to the channel generator.
  pure def proposer_for(height: Height, round: Round): Node =
    NODES[(height - 1 + round) % NODES.length()]

  import channel_api_generator(NODES = NODES, proposer_for = proposer_for) as gen from "channel_api_generator"
  import engine_api_generator(NODES = NODES) as reth from "engine_api_generator"

  // ===========================================================================
  // EMERALD-SPECIFIC TYPES (disk/mem split, extended with Engine API tracking)
  // ===========================================================================

  type AppPhase =
    | Uninitialized  // Before ConsensusReady
    | Ready          // After ConsensusReady or Decided, before StartedRound
    | Working        // After StartedRound
    | Syncing        // After ProcessSyncedValue

  // ---------------------------------------------------------------------------
  // Non-atomic Engine API types
  // ---------------------------------------------------------------------------

  // Specifies which Engine API primitive action to call.
  // One variant per primitive action, carrying required parameters.
  // TODO: why is this not part of the engine generator?
  type EngineCallSpec =
    | CallBuildRequest                                            // respondBuildRequest(node)
    | CallGetPayload                                              // respondGetPayload(node)
    | CallNewPayload({ block: eac::Block })                       // respondNewPayload(node, block)
    | CallHeadUpdate({ headHash: eac::BlockHash,
                       headHeight: eac::Height })                 // respondHeadUpdate(node, headHash, headHeight)

  // Handler-specific context carried through the queue for finalization.
  // Each variant holds exactly the data the corresponding handler needs.
  type ProcessingContext =
    | ProcGetValue({ proposal: Proposal })
    | ProcReceivedProposal({ proposal: Proposal, block: eac::Block })
    | ProcDecided({ proposal: Proposal, block: eac::Block })

  // Pending Engine API work: a list of remaining calls + context for finalization.
  // When remainingCalls is empty, finalize using context.
  type PendingWork = {
    remainingCalls: List[EngineCallSpec],
    context: ProcessingContext,
  }

  // ---------------------------------------------------------------------------
  // Emerald node state
  // ---------------------------------------------------------------------------

  // Disk state: survives restart, lost on crash
  type EmeraldDiskState = {
    decided_proposals: Set[Proposal],  // committed decisions
    last_decided_height: Height,
    last_decided_payload: Option[Payload],
    // Engine API tracking (persisted)
    latest_block: Option[eac::Block],  // Latest committed block
  }

  // Mem state: cleared on any fault (crash or restart)
  type EmeraldMemState = {
    phase: AppPhase,
    consensus_height: Height,
    consensus_round: Round,
    pending_proposals: Set[Proposal],    // height > consensus_height
    undecided_proposals: Set[Proposal],  // height == consensus_height
    // Engine API tracking (ephemeral)
    head_block_hash: eac::BlockHash,     // Current execution head
    validated_cache: Set[eac::BlockHash], // Cached validation results
    // Non-atomic Engine API processing queue
    pendingWork: Option[PendingWork],
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
    decided_proposals: Set(),
    last_decided_height: 0,
    last_decided_payload: None,
    latest_block: None,
  }

  pure val initEmeraldMem: EmeraldMemState = {
    phase: Uninitialized,
    consensus_height: 0,
    consensus_round: 0,
    pending_proposals: Set(),
    undecided_proposals: Set(),
    head_block_hash: 0,
    validated_cache: Set(),
    pendingWork: None,
  }

  pure val initEmeraldNode: EmeraldNodeState = {
    disk: initEmeraldDisk,
    mem: initEmeraldMem,
  }

  action init = all {
    gen::init,
    reth::init,
    emeraldState' = NODES.toSet().mapBy(n => initEmeraldNode),
  }

  // ===========================================================================
  // HANDLERS: Emerald's response to Channel API messages
  // ===========================================================================

  // ConsensusReady: Initialize node.
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
  action handleStartedRound(node: Node, height: Height, round: Round): bool = {
    val s = emeraldState.get(node)
    val promoted = s.mem.pending_proposals.filter(p => p.height == height)
    val remaining = s.mem.pending_proposals.exclude(promoted)
    emeraldState' = emeraldState.set(node, {
      ...s,
      mem: {
        ...s.mem,
        phase: Working,
        consensus_height: height,
        consensus_round: round,
        pending_proposals: remaining,
        undecided_proposals: s.mem.undecided_proposals.union(promoted),
      },
    })
  }

  // GetValue: Proposer builds a value using Engine API.
  // Called during finalization (after respondBuildRequest + respondGetPayload).
  action handleGetValue(node: Node, proposal: Proposal): bool = {
    val s = emeraldState.get(node)
    emeraldState' = emeraldState.set(node, {
      ...s,
      mem: {
        ...s.mem,
        undecided_proposals: s.mem.undecided_proposals.union(Set(proposal)),
        pendingWork: None,
      },
    })
  }

  // ReceivedProposal: Non-proposer learns about a proposal.
  // Validates via Engine API (newPayload). Block tracking updated.
  action handleReceivedProposal(node: Node, proposal: Proposal, blockHash: eac::BlockHash, valid: bool): bool = {
    val s = emeraldState.get(node)
    if (proposal.height < s.mem.consensus_height) {
      // Stale proposal — drop it
      emeraldState' = emeraldState
    } else if (proposal.height == s.mem.consensus_height) {
      // Current height — actionable (called from finalization)
      emeraldState' = emeraldState.set(node, {
        ...s,
        mem: {
          ...s.mem,
          undecided_proposals: s.mem.undecided_proposals.union(Set(proposal)),
          validated_cache: if (valid) s.mem.validated_cache.union(Set(blockHash)) else s.mem.validated_cache,
          pendingWork: None,
        },
      })
    } else {
      // Future height — can't validate yet
      emeraldState' = emeraldState.set(node, {
        ...s,
        mem: { ...s.mem, pending_proposals: s.mem.pending_proposals.union(Set(proposal)) },
      })
    }
  }

  // Decided: Consensus decided on a value.
  // Called during finalization (after respondNewPayload + respondHeadUpdate).
  action handleDecided(node: Node, proposal: Proposal, decidedBlock: eac::Block): bool = {
    val s = emeraldState.get(node)
    val nextHeight = proposal.height + 1
    emeraldState' = emeraldState.set(node, {
      disk: {
        ...s.disk,
        last_decided_height: proposal.height,
        last_decided_payload: Some(proposal.payload),
        decided_proposals: s.disk.decided_proposals.union(Set(proposal)),
        latest_block: Some(decidedBlock),
      },
      mem: {
        phase: Ready,
        consensus_height: nextHeight,
        consensus_round: 0,
        pending_proposals: s.mem.pending_proposals,
        undecided_proposals: Set(),
        head_block_hash: decidedBlock.hash,
        validated_cache: s.mem.validated_cache.union(Set(decidedBlock.hash)),
        pendingWork: None,
      },
    })
  }

  // ProcessSyncedValue: Catch-up with a synced decided value.
  // No Engine API calls — validation deferred to Decided.
  action handleProcessSyncedValue(node: Node, proposal: Proposal): bool = {
    val s = emeraldState.get(node)
    emeraldState' = emeraldState.set(node, {
      ...s,
      mem: { ...s.mem, phase: Syncing, undecided_proposals: s.mem.undecided_proposals.union(Set(proposal)) },
    })
  }

  // GetDecidedValue: Query for past decided value (no Emerald state change)
  action handleGetDecidedValue(node: Node, height: Height): bool =
    emeraldState' = emeraldState

  // NodeCrash: Full reset — all state lost (disk + mem).
  action handleNodeCrash(node: Node): bool =
    emeraldState' = emeraldState.set(node, initEmeraldNode)

  // NodeRestart: Preserve disk state, reset mem.
  action handleNodeRestart(node: Node): bool = {
    val s = emeraldState.get(node)
    emeraldState' = emeraldState.set(node, { disk: s.disk, mem: initEmeraldMem })
  }

  // ===========================================================================
  // PROPOSAL CONSTRUCTION (same as emerald_with_generator)
  // ===========================================================================

  pure def buildProposal(s: EmeraldNodeState, node: Node): Proposal = {
    val existing = s.mem.undecided_proposals.find(p =>
      p.height == s.mem.consensus_height and p.round == s.mem.consensus_round
    )
    match existing {
      | Some(p) => p
      | None => {
          height: s.mem.consensus_height,
          round: s.mem.consensus_round,
          proposer: node,
          payload: s.mem.consensus_height * 100 + s.mem.consensus_round,
        }
    }
  }

  // ===========================================================================
  // BLOCK CONSTRUCTION (for Engine API calls)
  // ===========================================================================

  // Block for validating a received/decided proposal
  pure def validationBlock(proposal: Proposal, parentHash: eac::BlockHash): eac::Block = {
    height: proposal.height,
    hash: proposal.payload * 10 + proposal.height,
    parentHash: parentHash,
  }

  // ===========================================================================
  // RETH STATE PASSTHROUGH (for steps with no Engine API calls)
  // ===========================================================================

  // Delegate to generator's published action (no need to know internal state vars)
  // TODO: why is this needed and we cannot use directly reth::rethUnchanged? 
  action rethUnchanged: bool = reth::rethUnchanged

  // ===========================================================================
  // NON-ATOMIC ENGINE API PROCESSING
  // ===========================================================================

  // Dispatch a single Engine API call to the appropriate Reth primitive action.
  // TODO: should this be part of the engine generator? 
  action dispatchEngineCall(node: Node, call: EngineCallSpec): bool =
    match call {
      | CallBuildRequest => reth::respondBuildRequest(node)
      | CallGetPayload => reth::respondGetPayload(node)
      | CallNewPayload(np) => reth::respondNewPayload(node, np.block)
      | CallHeadUpdate(hu) => reth::respondHeadUpdate(node, hu.headHash, hu.headHeight)
    }

  // Finalize processing by calling the appropriate Emerald handler.
  // Called when all Engine API calls in the queue have been executed.
  action finalizeProcessing(node: Node, pw: PendingWork): bool =
    match pw.context {
      | ProcGetValue(gv) => handleGetValue(node, gv.proposal)
      | ProcReceivedProposal(rp) =>
          val rns = reth::rethStates.get(node)
          val valid = reth::computePayloadStatus(rns, rp.block) == eac::Valid
          handleReceivedProposal(node, rp.proposal, rp.block.hash, valid)
      | ProcDecided(d) => handleDecided(node, d.proposal, d.block)
    }

  // Process one Engine API call from the queue, or finalize if queue is empty.
  // TODO: why is not finalizeProcessing one of the pending work items? 
  action stepAdvanceEngine(node: Node): bool = {
    val s = emeraldState.get(node)
    // Extract with defaults — guard below ensures pendingWork is Some
    val pw: PendingWork = match s.mem.pendingWork {
      | Some(pw) => pw
      | None => {
          remainingCalls: [],
          context: ProcGetValue({ proposal: { height: 0, round: 0, proposer: "", payload: 0 } }),
        }
    }
    all {
      s.mem.pendingWork != None,
      if (pw.remainingCalls.length() > 0)
        all {
          dispatchEngineCall(node, pw.remainingCalls.head()),
          gen::genUnchanged,
          emeraldState' = emeraldState.set(node, {
            ...s,
            mem: { ...s.mem, pendingWork: Some({ ...pw, remainingCalls: pw.remainingCalls.tail() }) },
          }),
        }
      else
        all {
          finalizeProcessing(node, pw),
          gen::genUnchanged,
          rethUnchanged,
        }
    }
  }

  // ===========================================================================
  // COMPOSED STEPS
  // ===========================================================================

  // ConsensusReady: Channel API only. Also start Reth if offline.
  // TODO: Model bootstrap logic on ConsensusReady.
  // The real implementation (bootstrap.rs) has two initialization paths:
  //   1. Genesis (no stored blocks): initialize_state_from_genesis fetches the
  //      genesis block from Reth, sets latest_block, and starts consensus at
  //      genesis height + 1.
  //   2. Existing block (restart with stored decisions): initialize_state_from_existing_block
  //      compares Reth's latest height against Emerald's stored height. If Reth
  //      is behind, replays decided blocks via newPayload + forkchoiceUpdated per
  //      height until aligned. Then sends a final forkchoiceUpdated to set the head,
  //      and starts consensus at stored height + 1.
  // Currently the spec skips all of this — handleConsensusReady just sets phase
  // to Ready without initializing latest_block or aligning Reth's chain state.
  // This means:
  //   - latest_block is None after ConsensusReady (genesis block not fetched)
  //   - Reth's head may be inconsistent with Emerald's disk after restart
  //   - No Engine API replay calls are modeled (newPayload + FCU per replayed height)
  //   - head_tracks_consensus invariant is only incidentally satisfied
  action stepConsensusReady(node: Node): bool = {
    val startHeight = emeraldState.get(node).disk.last_decided_height + 1
    val rns = reth::rethStates.get(node)
    all {
      emeraldState.get(node).mem.pendingWork == None,
      gen::sendConsensusReady(node, startHeight),
      handleConsensusReady(node, startHeight),
      // Start Reth if not already running
      // TODO (low priority) avoid accessing rns state 
      if (rns.mem.phase == reth::Offline) reth::rethStart(node)
      else rethUnchanged,
    }
  }

  // StartedRound: Channel API only, no Engine API calls.
  // TODO: Add Engine API validation for promoted proposals
  // Currently, pending proposals are promoted to undecided WITHOUT validation via
  // newPayload. In the real implementation, StartedRound validates pending proposals
  // at the new height before promoting them. This causes:
  //   1. Missing Engine API calls (not matching implementation)
  //   2. validated_before_decided invariant is trivial (validation happens in stepDecided)
  //   3. Later validation failures that could be caught earlier
  //
  // Fix: Add validatePromotedProposals(node, promoted) before handleStartedRound
  // to validate pending proposals via newPayload (mem only, no disk chain updates).
  // See plan file: /home/dev/.claude/plans/shimmering-bubbling-breeze.md
  action stepStartedRound(node: Node): bool = {
    val s = emeraldState.get(node).mem
    all {
      emeraldState.get(node).mem.pendingWork == None,
      any {
        all {
          gen::sendStartedRound(node),
          handleStartedRound(node, s.consensus_height, s.consensus_round),
        },
        all {
          gen::sendStartedRoundAfterTimeout(node),
          handleStartedRound(node, s.consensus_height, s.consensus_round + 1),
        },
      },
      rethUnchanged,
    }
  }

  // GetValue: Receive Channel API event, enqueue [BuildRequest, GetPayload]
  // for Engine API processing. Handler fires during finalization.
  action stepGetValue(node: Node): bool = {
    val s = emeraldState.get(node)
    val proposal = buildProposal(s, node)
    all {
      s.mem.pendingWork == None,
      gen::sendGetValue(node, proposal),
      rethUnchanged,
      emeraldState' = emeraldState.set(node, {
        ...s,
        mem: { ...s.mem,
          // TODO does this pattern work also if CallGetPayload depends on the response of CallBuildRequest? Or is this response stored in the engine generator?
          pendingWork: Some({
            remainingCalls: [CallBuildRequest, CallGetPayload],
            context: ProcGetValue({ proposal: proposal }),
          }),
        },
      }),
    }
  }

  // ReceivedProposal: Receive Channel API event. For current-height proposals,
  // enqueue [NewPayload] for Engine API validation. For future-height proposals,
  // handle immediately (buffer as pending, no Engine API needed).
  action stepReceivedProposal(node: Node, proposal: Proposal): bool = {
    val s = emeraldState.get(node)
    // TODO (low priority) avoid accessing rns state 
    val rns = reth::rethStates.get(node)
    val parentHash = if (rns.disk.chain.has(proposal.height - 1))
      rns.disk.chain.get(proposal.height - 1).hash
    else
      -1
    val block: eac::Block = validationBlock(proposal, parentHash)
    all {
      s.mem.pendingWork == None,
      gen::sendReceivedProposal(node, proposal),
      rethUnchanged,
      // TODO what if the proposal is for an old height; the channel API generator drops them, but we should still cover that edge case here
      emeraldState' = if (proposal.height == s.mem.consensus_height)
        // Current height — enqueue Engine API validation
        emeraldState.set(node, { ...s, mem: { ...s.mem,
          pendingWork: Some({
            remainingCalls: [CallNewPayload({ block: block })],
            context: ProcReceivedProposal({ proposal: proposal, block: block }),
          }),
        }})
      else
        // Future height — handle immediately, no Engine API needed
        emeraldState.set(node, { ...s, mem: { ...s.mem,
          pending_proposals: s.mem.pending_proposals.union(Set(proposal)),
        }}),
    }
  }

  // Decided: Receive Channel API event, enqueue [NewPayload, HeadUpdate]
  // for Engine API processing. Handler fires during finalization.
  // Guard: block must be valid (prevents enqueueing uncompleable work).
  action stepDecided(node: Node, proposal: Proposal): bool = {
    val s = emeraldState.get(node)
    // TODO (low priority) avoid accessing rns state 
    val rns = reth::rethStates.get(node)
    val parentHash = if (rns.disk.chain.has(proposal.height - 1))
      rns.disk.chain.get(proposal.height - 1).hash
    else
      -1
    val block: eac::Block = validationBlock(proposal, parentHash)
    all {
      s.mem.pendingWork == None,
      // Guard: block must be valid (same as atomic respondValidateAndFinalize's guard)
      reth::computePayloadStatus(rns, block) == eac::Valid,
      gen::sendDecided(node, proposal),
      rethUnchanged,
      emeraldState' = emeraldState.set(node, { ...s, mem: { ...s.mem,
        pendingWork: Some({
          remainingCalls: [
            CallNewPayload({ block: block }),
            CallHeadUpdate({ headHash: block.hash, headHeight: block.height }),
          ],
          context: ProcDecided({ proposal: proposal, block: block }),
        }),
      }}),
    }
  }

  // ProcessSyncedValue: Channel API only, no Engine API calls.
  action stepProcessSyncedValue(node: Node): bool = {
    val h = emeraldState.get(node).mem.consensus_height
    val proposal = gen::decisions.get(h)
    all {
      emeraldState.get(node).mem.pendingWork == None,
      gen::sendProcessSyncedValue(node),
      handleProcessSyncedValue(node, proposal),
      rethUnchanged,
    }
  }

  // GetDecidedValue: Channel API only, no Engine API calls.
  action stepGetDecidedValue(node: Node, height: Height): bool = all {
    emeraldState.get(node).mem.pendingWork == None,
    gen::sendGetDecidedValue(node, height),
    handleGetDecidedValue(node, height),
    rethUnchanged,
  }

  // NodeCrash: Both generators + Emerald state reset.
  // No idle guard — faults can occur mid-processing (clears pendingWork).
  // TODO: Model independent process crashes
  // Currently crashes BOTH Emerald and Reth together, but they run in separate
  // processes and can crash independently. Should add:
  //   - stepEmeraldCrash: crash Emerald only (Reth keeps running)
  //   - stepRethCrash: crash Reth only (Emerald keeps running)
  //   - stepBothCrash: crash both (current behavior)
  //
  // Independent crashes test more realistic failure scenarios:
  //   - Emerald crashes: loses mem state (pending, undecided, validated_cache),
  //     but Reth continues validating; when Emerald restarts, must resync
  //   - Reth crashes: loses mem state (validatedBlocks, pendingBuild), but
  //     Emerald continues; subsequent Engine API calls may fail or require retry
  //   - Both crash: current model (coordinated failure)
  //
  // Impact on invariants:
  //   - validated_before_decided: must check Reth DISK chain (mem cleared on Reth crash)
  //   - head_tracks_consensus: Emerald may be ahead if Reth crashed and restarted
  action stepNodeCrash(node: Node): bool = all {
    gen::nodeCrash(node),
    reth::rethCrash(node),
    handleNodeCrash(node),
  }

  // NodeRestart: Both generators + Emerald state reset (disk preserved).
  // No idle guard — faults can occur mid-processing (clears pendingWork).
  // TODO: Same as stepNodeCrash - should model independent restarts
  action stepNodeRestart(node: Node): bool = all {
    gen::nodeRestart(node),
    reth::rethRestart(node),
    handleNodeRestart(node),
  }

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
      stepAdvanceEngine(node),
      stepNodeCrash(node),
      stepNodeRestart(node),
    }
  }

  // ===========================================================================
  // INVARIANTS
  // ===========================================================================

  // TODO This should be Emerald specific invariants, with an option to run this spec against both contracts

  // --- Channel API invariants ---

  // Safety: All nodes agree on decided blocks.
  val emerald_agreement =
    NODES.toSet().forall(n1 =>
      NODES.toSet().forall(n2 =>
        val st1 = emeraldState.get(n1)
        val st2 = emeraldState.get(n2)
        st1.disk.last_decided_height == st2.disk.last_decided_height implies
          st1.disk.last_decided_payload == st2.disk.last_decided_payload
      )
    )

  // Consistency: consensus height > last decided height after initialization.
  val consensus_height_gt_last_decided_height =
    NODES.toSet().forall(n =>
      val st = emeraldState.get(n)
      st.mem.consensus_height > 0 implies
        st.mem.consensus_height > st.disk.last_decided_height
    )

  // Completeness: All nodes know all decided proposals.
  val completion =
    NODES.toSet().forall(n =>
      val st = emeraldState.get(n)
      1.to(st.disk.last_decided_height).forall(height =>
        gen::decisions.has(height) and gen::decisions.get(height).in(st.disk.decided_proposals)
      )
    )

  // Lifecycle: No pending proposals at current height.
  val no_pending_at_current_height =
    NODES.toSet().forall(n =>
      val st = emeraldState.get(n)
      st.mem.consensus_height > 0 implies
        st.mem.pending_proposals.forall(p => p.height > st.mem.consensus_height)
    )

  // --- Engine API invariants ---

  // head_tracks_consensus: After Decided(h), Reth's head height >= h.
  val head_tracks_consensus =
    NODES.toSet().forall(n =>
      val st = emeraldState.get(n)
      val rns = reth::rethStates.get(n)
      st.disk.last_decided_height > 0 and st.mem.phase != Uninitialized implies
        rns.disk.headHeight >= st.disk.last_decided_height
    )

  // validated_before_decided: Every decided block was validated.
  // Checks mem (validated_cache, reth validatedBlocks) and disk (reth chain).
  // After restart, mem evidence is cleared but the block persists in Reth's chain.
  val validated_before_decided =
    NODES.toSet().forall(n =>
      val st = emeraldState.get(n)
      val rns = reth::rethStates.get(n)
      match st.disk.latest_block {
        | Some(b) =>
            // Mem evidence (current session)
            st.mem.validated_cache.contains(b.hash) or
            rns.mem.validatedBlocks.contains(b.hash) or
            // Disk evidence (survives restart)
            rns.disk.chain.keys().exists(h => rns.disk.chain.get(h).hash == b.hash)
        | None => true
      }
    )

  // chain_continuity: Reth's canonical chain forms a linked list.
  val chain_continuity_inv =
    NODES.toSet().forall(n =>
      val rns = reth::rethStates.get(n)
      rns.disk.chain.keys().forall(h =>
        h == 0 or {
          rns.disk.chain.has(h - 1) and
          rns.disk.chain.get(h).parentHash == rns.disk.chain.get(h - 1).hash
        }
      )
    )
}
