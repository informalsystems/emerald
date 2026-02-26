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

// TODO create README file with description of the composition approach, conventions, future work
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
  //
  // TODO: EngineCallSpec and dispatchEngineCall are boilerplate — mechanically
  // derivable from the generator's action signatures. They belong in (or next
  // to) the engine generator as reusable consumer infrastructure, and are
  // candidates for language-level automation (Quint could auto-generate a call
  // enum + dispatcher when a spec imports a generator).
  type EngineCallSpec =
    | CallBuildRequest                                            // respondBuildRequest(node)
    | CallGetPayload                                              // respondGetPayload(node)
    | CallNewPayload({ block: eac::Block })                       // respondNewPayload(node, block)
    | CallHeadUpdate({ headHash: eac::BlockHash,
                       headHeight: eac::Height })                 // respondHeadUpdate(node, headHash, headHeight)

  // A single unit of work in the node's processing queue.
  // EngineCall: external calls to Reth (generic, any consumer would have the same).
  // FinalizeX: local Emerald state updates after engine work completes (app-specific).
  // Generalizes naturally: an app talking to multiple external components would
  // add variants for each (e.g., MempoolCall, OracleCall) interleaved with local logic.
  //
  // TODO: Cleaner state separation between Emerald and generators.
  // Currently Emerald reads reth::rethStates directly in several places:
  //   (a) Block construction: reads reth.disk.chain for parentHash at enqueue time
  //       (stepReceivedProposal, stepDecided)
  //   (b) Validity pre-check: computePayloadStatus guard at enqueue time (stepDecided)
  //   (c) Cross-component data flow: FinalizeReceivedProposal reads reth state to get
  //       validation result from preceding CallNewPayload
  //   (d) Phase check: stepConsensusReady checks reth phase to decide rethStart
  // To eliminate this coupling:
  //   - EngineCall dispatch could write responses into Emerald's own state (e.g.,
  //     lastResponse: Option[EngineResponse]), so handlers read local state not
  //     generator state. This fixes (c).
  //   - Block construction could become a work item itself (e.g.,
  //     BuildValidationBlock) that reads Reth state and enqueues CallNewPayload.
  //     This fixes (a) and (b).
  //   - Phase check could use an observable flag published by the generator. Fixes (d).
  type WorkItem =
    | EngineCall(EngineCallSpec)
    | FinalizeGetValue({ proposal: Proposal })
    | FinalizeReceivedProposal({ proposal: Proposal, block: eac::Block })
    | FinalizeDecided({ proposal: Proposal, block: eac::Block })

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
    // Sequential work queue: engine API calls + local handlers
    pendingWork: List[WorkItem],
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
    pendingWork: [],
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

  // Update Emerald state after Engine API build (respondBuildRequest + respondGetPayload).
  // Adds the proposer's proposal to undecided_proposals.
  action finalizeGetValue(node: Node, proposal: Proposal, remaining: List[WorkItem]): bool = {
    val s = emeraldState.get(node)
    emeraldState' = emeraldState.set(node, {
      ...s,
      mem: {
        ...s.mem,
        undecided_proposals: s.mem.undecided_proposals.union(Set(proposal)),
        pendingWork: remaining,
      },
    })
  }

  // Update Emerald state after Engine API validation (respondNewPayload).
  // Adds proposal to undecided_proposals and caches validation result.
  // Only called for current-height proposals — stale and future proposals
  // are filtered at enqueue time in stepReceivedProposal.
  action finalizeReceivedProposal(node: Node, proposal: Proposal, blockHash: eac::BlockHash, valid: bool, remaining: List[WorkItem]): bool = {
    val s = emeraldState.get(node)
    emeraldState' = emeraldState.set(node, {
      ...s,
      mem: {
        ...s.mem,
        undecided_proposals: s.mem.undecided_proposals.union(Set(proposal)),
        validated_cache: if (valid) s.mem.validated_cache.union(Set(blockHash)) else s.mem.validated_cache,
        pendingWork: remaining,
      },
    })
  }

  // Update Emerald state after Engine API finalization (respondNewPayload + respondHeadUpdate).
  // Commits decided proposal to disk, advances to next height, updates head.
  action finalizeDecided(node: Node, proposal: Proposal, decidedBlock: eac::Block, remaining: List[WorkItem]): bool = {
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
        pendingWork: remaining,
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
  // WORK QUEUE PROCESSING
  // ===========================================================================

  // Dispatch a single Engine API call to the appropriate Reth primitive action.
  // TODO: boilerplate — see EngineCallSpec TODO above.
  action dispatchEngineCall(node: Node, call: EngineCallSpec): bool =
    match call {
      | CallBuildRequest => reth::respondBuildRequest(node)
      | CallGetPayload => reth::respondGetPayload(node)
      | CallNewPayload(np) => reth::respondNewPayload(node, np.block)
      | CallHeadUpdate(hu) => reth::respondHeadUpdate(node, hu.headHash, hu.headHeight)
    }

  // Process the next work item from the queue.
  // Pops the head, dispatches it, and advances the queue in one step.
  action stepAdvanceWork(node: Node): bool = {
    val s = emeraldState.get(node)
    val remaining = s.mem.pendingWork.tail()
    all {
      s.mem.pendingWork.length() > 0,
      match s.mem.pendingWork.head() {
        | EngineCall(call) =>
            all {
              dispatchEngineCall(node, call),
              gen::genUnchanged,
              emeraldState' = emeraldState.set(node, {
                ...s,
                mem: { ...s.mem, pendingWork: remaining },
              }),
            }
        | FinalizeGetValue(gv) =>
            all {
              reth::rethUnchanged,
              gen::genUnchanged,
              finalizeGetValue(node, gv.proposal, remaining),
            }
        | FinalizeReceivedProposal(rp) =>
            // Cross-component data flow: read Reth state to get the validation
            // result from the preceding CallNewPayload (which wrote to rethStates).
            val rns = reth::rethStates.get(node)
            val valid = reth::computePayloadStatus(rns, rp.block) == eac::Valid
            all {
              reth::rethUnchanged,
              gen::genUnchanged,
              finalizeReceivedProposal(node, rp.proposal, rp.block.hash, valid, remaining),
            }
        | FinalizeDecided(d) =>
            all {
              reth::rethUnchanged,
              gen::genUnchanged,
              finalizeDecided(node, d.proposal, d.block, remaining),
            }
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
      emeraldState.get(node).mem.pendingWork.length() == 0,
      gen::sendConsensusReady(node, startHeight),
      handleConsensusReady(node, startHeight),
      // Start Reth if not already running
  
      if (rns.mem.phase == reth::Offline) reth::rethStart(node)
      else reth::rethUnchanged,
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
      emeraldState.get(node).mem.pendingWork.length() == 0,
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
      reth::rethUnchanged,
    }
  }

  // GetValue: Receive Channel API event, enqueue [BuildRequest, GetPayload]
  // for Engine API processing. Handler fires during finalization.
  action stepGetValue(node: Node): bool = {
    val s = emeraldState.get(node)
    val proposal = buildProposal(s, node)
    all {
      s.mem.pendingWork.length() == 0,
      gen::sendGetValue(node, proposal),
      reth::rethUnchanged,
      emeraldState' = emeraldState.set(node, {
        ...s,
        mem: { ...s.mem,
          // Work items don't pass data to each other directly — they communicate
          // through observable component state. The queue guarantees ordering so
          // reads see prior writes. E.g., CallGetPayload reads the pendingBuild
          // that CallBuildRequest wrote to rethStates; FinalizeGetValue could read
          // the built block from reth::rethStates.get(node).disk.chain if needed
          // (as FinalizeReceivedProposal already does for validation).
          pendingWork: [
            EngineCall(CallBuildRequest),
            EngineCall(CallGetPayload),
            FinalizeGetValue({ proposal: proposal }),
          ],
        },
      }),
    }
  }

  // ReceivedProposal: Receive Channel API event. For current-height proposals,
  // enqueue [NewPayload] for Engine API validation. For future-height proposals,
  // handle immediately (buffer as pending, no Engine API needed).
  action stepReceivedProposal(node: Node, proposal: Proposal): bool = {
    val s = emeraldState.get(node)

    val rns = reth::rethStates.get(node)
    val parentHash = if (rns.disk.chain.has(proposal.height - 1))
      rns.disk.chain.get(proposal.height - 1).hash
    else
      -1
    val block: eac::Block = validationBlock(proposal, parentHash)
    all {
      s.mem.pendingWork.length() == 0,
      gen::sendReceivedProposal(node, proposal),
      reth::rethUnchanged,
      emeraldState' = if (proposal.height < s.mem.consensus_height)
        // Stale proposal — drop it, no Engine API call.
        // Currently unreachable (generator guards height >= node.height),
        // but defensive for when independent crashes are modeled.
        emeraldState
      else if (proposal.height == s.mem.consensus_height)
        // Current height — enqueue Engine API validation + local handler
        emeraldState.set(node, { ...s, mem: { ...s.mem,
          pendingWork: [
            EngineCall(CallNewPayload({ block: block })),
            FinalizeReceivedProposal({ proposal: proposal, block: block }),
          ],
        }})
      else
        // Future height — buffer as pending, no Engine API needed
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

    val rns = reth::rethStates.get(node)
    val parentHash = if (rns.disk.chain.has(proposal.height - 1))
      rns.disk.chain.get(proposal.height - 1).hash
    else
      -1
    val block: eac::Block = validationBlock(proposal, parentHash)
    all {
      s.mem.pendingWork.length() == 0,
      // Guard: block must be valid (same as atomic respondValidateAndFinalize's guard)
      reth::computePayloadStatus(rns, block) == eac::Valid,
      gen::sendDecided(node, proposal),
      reth::rethUnchanged,
      emeraldState' = emeraldState.set(node, { ...s, mem: { ...s.mem,
        pendingWork: [
          EngineCall(CallNewPayload({ block: block })),
          EngineCall(CallHeadUpdate({ headHash: block.hash, headHeight: block.height })),
          FinalizeDecided({ proposal: proposal, block: block }),
        ],
      }}),
    }
  }

  // ProcessSyncedValue: Channel API only, no Engine API calls.
  action stepProcessSyncedValue(node: Node): bool = {
    val h = emeraldState.get(node).mem.consensus_height
    val proposal = gen::decisions.get(h)
    all {
      emeraldState.get(node).mem.pendingWork.length() == 0,
      gen::sendProcessSyncedValue(node),
      handleProcessSyncedValue(node, proposal),
      reth::rethUnchanged,
    }
  }

  // GetDecidedValue: Channel API only, no Engine API calls.
  action stepGetDecidedValue(node: Node, height: Height): bool = all {
    emeraldState.get(node).mem.pendingWork.length() == 0,
    gen::sendGetDecidedValue(node, height),
    handleGetDecidedValue(node, height),
    reth::rethUnchanged,
  }

  // NodeCrash: Both generators + Emerald state reset.
  // No idle guard — faults can occur mid-processing (clears pendingWork).
  //
  // TODO: Model independent process crashes.
  // Malachite+Emerald run in one process, Reth in another. Should replace
  // stepNodeCrash/stepNodeRestart with 4 independent actions:
  //
  //   - stepEmeraldCrash/Restart: gen::nodeCrash/Restart + handleNodeCrash/Restart,
  //     reth::rethUnchanged. Emerald+Malachite fail, Reth keeps running.
  //
  //   - stepRethCrash: reth::rethCrash, gen::genUnchanged. Must also clear
  //     Emerald's pendingWork (engine calls can't execute against Offline Reth,
  //     and rethStart requires empty queue — deadlock otherwise) and
  //     validated_cache (references blocks no longer in Reth's chain).
  //
  //   - stepRethRestart: reth::rethRestart, gen::genUnchanged. Must clear
  //     Emerald's pendingWork (pendingBuild is gone, queued CallGetPayload
  //     would fail). validated_cache can stay (disk chain preserved).
  //
  // Invariant impact:
  //   - head_tracks_consensus: after Reth crash, headHeight resets to 0 while
  //     last_decided_height stays. Must gate on reth phase != Offline.
  //   - validated_before_decided: after Reth crash, all evidence (mem + disk)
  //     is gone. Must gate on reth phase != Offline.
  //   - chainContinuity (reth contract): uses disk state only, robust to all crash types.
  //
  // No "both crash" convenience action needed — stepEmeraldCrash followed by
  // stepRethCrash in consecutive steps achieves the same result.
  action stepNodeCrash(node: Node): bool = all {
    gen::nodeCrash(node),
    reth::rethCrash(node),
    handleNodeCrash(node),
  }

  // NodeRestart: Both generators + Emerald state reset (disk preserved).
  // No idle guard — faults can occur mid-processing (clears pendingWork).
  // TODO: See stepNodeCrash TODO — same independent crash split applies.
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
      stepAdvanceWork(node),
      stepNodeCrash(node),
      stepNodeRestart(node),
    }
  }

  // ===========================================================================
  // INVARIANTS
  // ===========================================================================

  // Invariants are organized by scope:
  //   - Emerald-only: properties of Emerald's own state
  //   - Emerald ↔ Malachite: properties relating Emerald state to Channel API generator state
  //   - Emerald ↔ Reth: properties relating Emerald state to Engine API generator state
  // The generators also expose contractInv (gen::contractInv, reth::contractInv)
  // which check the full contract properties. These can be uncommented below
  // to verify the composition doesn't violate either contract.

  // --- Emerald-only invariants ---

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

  // Lifecycle: No pending proposals at current height.
  val no_pending_at_current_height =
    NODES.toSet().forall(n =>
      val st = emeraldState.get(n)
      st.mem.consensus_height > 0 implies
        st.mem.pending_proposals.forall(p => p.height > st.mem.consensus_height)
    )

  // --- Cross-boundary invariants (Emerald ↔ Malachite) ---

  // Completeness: All nodes know all decided proposals.
  val completion =
    NODES.toSet().forall(n =>
      val st = emeraldState.get(n)
      1.to(st.disk.last_decided_height).forall(height =>
        gen::decisions.has(height) and gen::decisions.get(height).in(st.disk.decided_proposals)
      )
    )

  // --- Cross-boundary invariants (Emerald ↔ Reth) ---

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

  // --- Generator contract invariants ---
  // Expensive: full contract property scans per step. Uncomment for
  // full verification that the composition doesn't violate either contract.
  // val channelContractInv = gen::contractInv
  // val engineContractInv = reth::contractInv

}
