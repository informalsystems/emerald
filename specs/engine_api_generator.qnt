// -*- mode: Bluespec; -*-

// =============================================================================
// Engine API Reference Generator (DRAFT)
// =============================================================================
//
// A state machine modeling Reth's Engine API responses. Verified against
// engine_api_contract.qnt.
//
// Unlike the Channel API generator (which nondeterministically picks what to
// send), this generator is REACTIVE — it responds to requests from Emerald.
// The caller (Emerald or a test driver) picks which Engine API call to make;
// the generator determines the response.
//
// Design:
//   - Per-node state tracks Reth's execution state (chain, builds in progress)
//   - Actions correspond to Engine API request-response pairs
//   - Response correctness is enforced by the state machine structure
//   - All contract properties are checked via contractInv at every step
//   - Fault injection: rethCrash (lose all state) and rethRestart (preserve
//     disk state) clear request histories and reset to Ready
//
// State follows the disk/mem convention from faults.qnt:
//   - Disk: chain, head, headHeight (survive restart)
//   - Mem: phase, validatedBlocks, pendingBuild (reset on fault)
//   - Request histories are mem state (cleared on fault)
//
// Phase 1 scope: Ready + Building phases. No SYNCING state.
//
// Block hash abstraction: deterministic hash = height * 1000 + index.
// Genesis block: height 0, hash 0, parentHash 0.

module engine_api_generator {
  import basicSpells.* from "spells/basicSpells"
  import rareSpells.* from "spells/rareSpells"
  import faults.* from "faults"
  import engine_api_contract.* from "engine_api_contract"

  // ===========================================================================
  // CONFIGURATION
  // ===========================================================================

  const NODES: List[Node]

  // Max height the generator will explore (bounds trace length)
  pure val MAX_HEIGHT = 4

  // ===========================================================================
  // PER-NODE STATE (disk/mem split)
  // ===========================================================================

  type RethPhase =
    | Offline   // Not running
    | Ready     // Processing requests normally
    | Building  // forkchoiceUpdated called with attrs, payload in progress

  // Disk state: survives restart, lost on crash
  type RethDiskState = {
    // Persisted canonical chain: height → block
    chain: Height -> Block,
    // Current head hash
    head: BlockHash,
    // Current head height
    headHeight: Height,
  }

  // Mem state: cleared on any fault (crash or restart)
  type RethMemState = {
    phase: RethPhase,
    // Blocks validated via newPayload in this session
    validatedBlocks: Set[BlockHash],
    // Pending build: payloadId + parentHash of the build
    pendingBuild: Option[{ payloadId: PayloadId, parentHash: BlockHash, parentHeight: Height }],
  }

  type RethNodeState = {
    disk: RethDiskState,
    mem: RethMemState,
  }

  // Genesis block: the foundation of the chain
  pure val GENESIS_BLOCK: Block = {
    height: 0,
    hash: 0,
    parentHash: 0,
  }

  pure val initRethDisk: RethDiskState = {
    chain: Map(0 -> GENESIS_BLOCK),
    head: 0,
    headHeight: 0,
  }

  pure val initRethMem: RethMemState = {
    phase: Offline,
    validatedBlocks: Set(),
    pendingBuild: None,
  }

  pure val readyRethMem: RethMemState = {
    phase: Ready,
    validatedBlocks: Set(),
    pendingBuild: None,
  }

  pure val initRethNode: RethNodeState = {
    disk: initRethDisk,
    mem: initRethMem,
  }

  // ===========================================================================
  // TRACE ENTRY (for lastEvent observation / MBT integration)
  // ===========================================================================

  type EngineTraceEntry =
    | EngineMsg(EngineRequest)
    | EngineFaultMsg(FaultEvent)

  // ===========================================================================
  // GLOBAL STATE
  // ===========================================================================

  // Per-node Reth state (disk/mem split)
  var rethStates: Node -> RethNodeState

  // Per-node request history (mem state)
  var requestHistory: Node -> List[EngineRequest]

  // Last trace entry emitted (for trace observation)
  var engineLastTraceEntry: Option[{ node: Node, entry: EngineTraceEntry }]

  // Counter for generating unique IDs (payload IDs and block hashes)
  var nextId: int

  // ===========================================================================
  // HELPERS
  // ===========================================================================

  // Deterministic hash: height * 1000 + a counter-like offset.
  pure def blockHash(height: Height, payloadId: PayloadId): BlockHash =
    height * 1000 + payloadId

  // Compute newPayload validation status for a block against a node's chain.
  // Extracted from respondNewPayload for reuse by composition (e.g., to pass
  // the validity result to Emerald's handleReceivedProposal).
  pure def computePayloadStatus(ns: RethNodeState, block: Block): PayloadStatusCode = {
    val parentValid = ns.disk.chain.has(block.height - 1) and
                      ns.disk.chain.get(block.height - 1).hash == block.parentHash
    if (parentValid) Valid else Invalid
  }

  // ===========================================================================
  // ACTIONS: Engine API request-response pairs
  // ===========================================================================
  //
  // Phase state machine:
  //
  //   Offline ─────────► Ready ◄──────────────┐
  //     (rethStart)        │                   │
  //                        │                   │
  //                        ▼                   │
  //                    Building ───────────────┘
  //                  (respondBuildRequest)  (respondGetPayload,
  //                                          respondHeadUpdate)
  //
  // Composition usage note:
  //   - In emerald_with_both_generators.qnt, respondBuildRequest + respondGetPayload
  //     are combined atomically in stepGetValue (instantaneous build model).
  //   - Standalone test driver explores these actions independently to validate
  //     contract properties under broader conditions than realistic usage.
  //
  // Implementation details vs contract properties:
  //   Guards and behaviors in these actions fall into three categories:
  //
  //   1. CONTRACT ENFORCEMENT: Guards that directly enforce contract properties
  //      Example: respondHeadUpdate checks block exists in chain (validBeforeHead)
  //
  //   2. IMPLEMENTATION DETAILS: Generator-specific choices not required by contract
  //      Example: respondBuildRequest always builds on current head (enables buildIntegrity
  //      but not the only way); phase state machine (internal, not observable)
  //
  //   3. OPTIMIZATIONS: Bounds that reduce search space without affecting correctness
  //      Example: block.height <= MAX_HEIGHT (real Reth handles arbitrary heights)
  //
  //   4. PHASE 1 CONSTRAINTS: Temporary simplifications to be relaxed in Phase 2
  //      Example: headMonotonic (no reorgs) — real Reth supports reorgs
  //
  //   Inline comments below mark guards with [CONTRACT], [IMPL], [OPT], or [PHASE1]
  //   to clarify their purpose.
  //

  // --- respondNewPayload ---
  // Emerald sends a block for validation. Reth validates and returns status.
  //
  // Valid if: block's parentHash matches the chain at (height - 1).
  // Invalid otherwise.
  //
  // Source: engine_newPayloadV4 in Engine API spec
  action respondNewPayload(node: Node, block: Block): bool = {
    val ns = rethStates.get(node)
    // [CONTRACT] validationDeterminism: validation outcome is deterministic based on chain state
    // This enables validationStability (once Valid, always Valid)
    val status = computePayloadStatus(ns, block)
    val parentValid = status == Valid
    val request =ReqNewPayload({ block: block, respStatus: status })
    all {
      // [IMPL] Phase check: internal state machine, not observable by contract
      or { ns.mem.phase == Ready, ns.mem.phase == Building },
      // [OPT] Height bounds: search space optimization (real Reth handles arbitrary heights)
      block.height >= 1,
      block.height <= MAX_HEIGHT,
      rethStates' = rethStates.set(node, {
        disk: {
          ...ns.disk,
          // Add valid blocks to the chain if no entry exists at that height.
          // Don't overwrite existing entries — prevents chainContinuity violations
          // when the standalone test validates different blocks at the same height.
          chain: if (parentValid and not(ns.disk.chain.has(block.height)))
            ns.disk.chain.put(block.height, block)
          else
            ns.disk.chain,
        },
        mem: {
          ...ns.mem,
          validatedBlocks: if (parentValid)
            ns.mem.validatedBlocks.union(Set(block.hash))
          else
            ns.mem.validatedBlocks,
        },
      }),
      requestHistory' = requestHistory.set(node,
        requestHistory.get(node).append(request)
      ),
      engineLastTraceEntry' = Some({ node: node, entry: EngineMsg(request) }),
      // Increment counter to ensure standalone test driver generates unique
      // block hashes (blockHash uses nextId). Without this, consecutive
      // respondNewPayload calls at the same height produce hash collisions.
      nextId' = nextId + 1,
    }
  }

  // --- respondBuildRequest ---
  // Emerald requests Reth to build a new block on top of the current head.
  // Generates a unique payloadId and enters Building phase.
  //
  // Key behaviors:
  //   - Always builds on ns.disk.head
  //   - Can be called while Building (overwrites pendingBuild, orphans old payloadId)
  //   - No disk changes (build persisted only when respondGetPayload is called)
  //
  // Composition: Combined atomically with respondGetPayload in stepGetValue.
  //
  // Source: engine_forkchoiceUpdatedV3 with payload attributes
  action respondBuildRequest(node: Node): bool = {
    val ns = rethStates.get(node)
    // [IMPL] buildOnCurrentHead: always builds on current head
    // One way to achieve buildIntegrity, but not the only way
    val headHash = ns.disk.head
    val request = ReqForkchoiceUpdated({
      headHash: headHash,
      building: true,
      respStatus: Valid,
      // [CONTRACT] payloadIdUniqueness: structural guarantee enabling buildLifecycle
      // nextId counter ensures globally unique payloadIds
      respPayloadId: Some(nextId),
    })
    all {
      // [IMPL] Phase check: internal state machine, not observable by contract
      or { ns.mem.phase == Ready, ns.mem.phase == Building },
      rethStates' = rethStates.set(node, {
        ...ns,
        mem: {
          phase: Building,
          validatedBlocks: ns.mem.validatedBlocks,
          pendingBuild: Some({ payloadId: nextId, parentHash: headHash, parentHeight: ns.disk.headHeight }),
        },
      }),
      requestHistory' = requestHistory.set(node,
        requestHistory.get(node).append(request)
      ),
      engineLastTraceEntry' = Some({ node: node, entry: EngineMsg(request) }),
      nextId' = nextId + 1,
    }
  }

  // --- respondHeadUpdate ---
  // Emerald updates Reth's canonical head after a consensus decision.
  // This finalizes a block as the active execution head without requesting a build.
  //
  // Key distinction from respondNewPayload/respondGetPayload:
  //   - Those actions ADD blocks to the chain (validation)
  //   - This action SELECTS which block is canonical (finalization)
  //
  // Source: engine_forkchoiceUpdatedV3 without payload attributes
  action respondHeadUpdate(node: Node, headHash: BlockHash, headHeight: Height): bool = {
    val ns = rethStates.get(node)
    val request = ReqForkchoiceUpdated({
      headHash: headHash,
      building: false,
      respStatus: Valid,
      respPayloadId: None,
    })
    all {
      // [IMPL] Phase check: internal state machine, not observable by contract
      or { ns.mem.phase == Ready, ns.mem.phase == Building },

      // [CONTRACT] validBeforeHead: block must already exist in chain before finalization
      ns.disk.chain.has(headHeight) and ns.disk.chain.get(headHeight).hash == headHash,

      // [PHASE1] headMonotonic: no reorgs constraint
      // EITHER: headHash == current head (idempotent update, allows retries)
      // OR: headHeight > current headHeight (monotonic forward progress)
      //
      // Why idempotency is needed:
      // - Consensus layer retries after network delays
      // - Re-establishment after restarts
      // - Duplicate Decided events in the model
      //
      // Phase 1 limitation: Prevents reorgs (can't move head from height 3 back to height 2)
      // Phase 2 will relax this to allow forking/reorg scenarios
      // NOTE: Real Reth supports reorgs — this is a modeling simplification
      or { headHash == ns.disk.head, headHeight > ns.disk.headHeight },

      // State updates: Only disk.head changes, chain itself is unchanged
      rethStates' = rethStates.set(node, {
        disk: {
          chain: ns.disk.chain,  // Chain unchanged (block already added earlier)
          head: headHash,        // Update canonical head
          headHeight: headHeight,
        },
        mem: {
          phase: Ready,  // Return to Ready (clear Building if active)
          validatedBlocks: ns.mem.validatedBlocks,
          pendingBuild: ns.mem.pendingBuild,
        },
      }),
      requestHistory' = requestHistory.set(node,
        requestHistory.get(node).append(request)
      ),
      engineLastTraceEntry' = Some({ node: node, entry: EngineMsg(request) }),
      nextId' = nextId,
    }
  }

  // --- respondGetPayload ---
  // Emerald retrieves the block built by a prior respondBuildRequest.
  // Returns the built block, adds it to chain, and clears pendingBuild.
  //
  // Lifecycle: PayloadId is single-use (respondBuildRequest generates, this consumes).
  //
  // Block construction:
  //   - Height: parentHeight + 1
  //   - Hash: deterministic (height * 1000 + payloadId)
  //   - ParentHash: from pendingBuild
  //
  // Match-with-defaults: Guards ensure pendingBuild is Some, None case never executes.
  //
  // Composition: Combined atomically with respondBuildRequest in stepGetValue.
  //
  // Source: engine_getPayloadV4 in Engine API spec
  action respondGetPayload(node: Node): bool = {
    val ns = rethStates.get(node)
    // Extract build info; guard ensures pendingBuild is Some
    val build = match ns.mem.pendingBuild {
      | Some(b) => b
      | None => { payloadId: -1, parentHash: -1, parentHeight: -1 }
    }
    val newBlock: Block = {
      height: build.parentHeight + 1,
      hash: blockHash(build.parentHeight + 1, build.payloadId),
      parentHash: build.parentHash,
    }
    val request =ReqGetPayload({ payloadId: build.payloadId, respBlock: newBlock })
    all {
      // [IMPL] getPayloadRequiresBuilding: stricter than contract requires
      // Contract only requires prior forkchoiceUpdated with this payloadId (buildLifecycle)
      // This guard adds phase ordering: must still be in Building state
      ns.mem.phase == Building,
      // [CONTRACT] buildLifecycle: payloadId must exist (part of single-use token property)
      ns.mem.pendingBuild != None,
      rethStates' = rethStates.set(node, {
        disk: {
          ...ns.disk,
          // Don't overwrite existing chain entries (same rationale as respondNewPayload)
          chain: if (not(ns.disk.chain.has(newBlock.height)))
            ns.disk.chain.put(newBlock.height, newBlock)
          else
            ns.disk.chain,
        },
        mem: {
          phase: Ready,
          validatedBlocks: ns.mem.validatedBlocks.union(Set(newBlock.hash)),
          pendingBuild: None,
        },
      }),
      requestHistory' = requestHistory.set(node,
        requestHistory.get(node).append(request)
      ),
      engineLastTraceEntry' = Some({ node: node, entry: EngineMsg(request) }),
      nextId' = nextId,
    }
  }

  // ===========================================================================
  // COMBINED ACTIONS (for composition)
  // ===========================================================================
  //
  // These actions combine primitive actions atomically for use in composition.
  // The composition calls these instead of manipulating generator state directly.
  // Primitive actions (respondBuildRequest, respondGetPayload, respondHeadUpdate)
  // remain for standalone testing.

  // --- respondBuildAndGetPayload ---
  // Atomically: forkchoiceUpdated(head, attrs=true) + getPayload(id).
  // Used by stepGetValue in composition. Instant build model — no Building
  // phase observable from outside.
  //
  // Constructs a new block on the current head, adds it to chain and
  // validatedBlocks, appends both FCU and GetPayload to history.
  action respondBuildAndGetPayload(node: Node): bool = {
    val ns = rethStates.get(node)
    val headHash = ns.disk.head
    val pid = nextId
    val newBlock: Block = {
      height: ns.disk.headHeight + 1,
      hash: blockHash(ns.disk.headHeight + 1, pid),
      parentHash: headHash,
    }
    val fcuReq = ReqForkchoiceUpdated({
      headHash: headHash,
      building: true,
      respStatus: Valid,
      respPayloadId: Some(pid),
    })
    val gpReq = ReqGetPayload({ payloadId: pid, respBlock: newBlock })
    all {
      // [IMPL] Phase check: internal state machine
      or { ns.mem.phase == Ready, ns.mem.phase == Building },
      rethStates' = rethStates.set(node, {
        disk: {
          ...ns.disk,
          // Don't overwrite existing chain entries (same rationale as respondNewPayload)
          chain: if (not(ns.disk.chain.has(newBlock.height)))
            ns.disk.chain.put(newBlock.height, newBlock)
          else
            ns.disk.chain,
        },
        mem: {
          phase: Ready,
          validatedBlocks: ns.mem.validatedBlocks.union(Set(newBlock.hash)),
          pendingBuild: None,
        },
      }),
      requestHistory' = requestHistory.set(node,
        requestHistory.get(node).append(fcuReq).append(gpReq)
      ),
      engineLastTraceEntry' = Some({ node: node, entry: EngineMsg(gpReq) }),
      nextId' = pid + 1,
    }
  }

  // --- respondValidateAndFinalize ---
  // Atomically: newPayload(block) + forkchoiceUpdated(block.hash, attrs=false).
  // Used by stepDecided in composition. Validates the decided block and updates
  // Reth's canonical head in one step.
  //
  // Guards: block must be valid (parent exists and matches), head must not
  // regress (Phase 1 no-reorg constraint).
  action respondValidateAndFinalize(node: Node, block: Block): bool = {
    val ns = rethStates.get(node)
    val status = computePayloadStatus(ns, block)
    val npReq = ReqNewPayload({ block: block, respStatus: status })
    val fcuReq = ReqForkchoiceUpdated({
      headHash: block.hash,
      building: false,
      respStatus: Valid,
      respPayloadId: None,
    })
    all {
      // [IMPL] Phase check: internal state machine
      or { ns.mem.phase == Ready, ns.mem.phase == Building },
      // [CONTRACT] Block must be valid (parent chain linkage)
      status == Valid,
      // [PHASE1] headMonotonic: no reorgs (same as respondHeadUpdate)
      or { block.hash == ns.disk.head, block.height > ns.disk.headHeight },
      rethStates' = rethStates.set(node, {
        disk: {
          chain: ns.disk.chain.put(block.height, block),
          head: block.hash,
          headHeight: block.height,
        },
        mem: {
          phase: Ready,
          validatedBlocks: ns.mem.validatedBlocks.union(Set(block.hash)),
          pendingBuild: None,
        },
      }),
      requestHistory' = requestHistory.set(node,
        requestHistory.get(node).append(npReq).append(fcuReq)
      ),
      engineLastTraceEntry' = Some({ node: node, entry: EngineMsg(fcuReq) }),
      // Increment counter (consistent with respondNewPayload)
      nextId' = nextId + 1,
    }
  }

  // --- rethUnchanged ---
  // Passthrough: no Engine API activity this step.
  // Exported for composition steps that don't interact with Reth.
  action rethUnchanged: bool = all {
    rethStates' = rethStates,
    requestHistory' = requestHistory,
    engineLastTraceEntry' = engineLastTraceEntry,
    nextId' = nextId,
  }

  // ===========================================================================
  // LIFECYCLE ACTIONS
  // ===========================================================================

  // --- rethStart ---
  // Start Reth on a node. Transitions from Offline to Ready.
  action rethStart(node: Node): bool = {
    val ns = rethStates.get(node)
    all {
      ns.mem.phase == Offline,
      rethStates' = rethStates.set(node, {
        ...ns,
        mem: readyRethMem,
      }),
      requestHistory' = requestHistory,
      engineLastTraceEntry' = engineLastTraceEntry,
      nextId' = nextId,
    }
  }

  // ===========================================================================
  // FAULT ACTIONS
  // ===========================================================================

  // --- rethCrash ---
  // Node lost all state: reset disk + mem, clear request history.
  action rethCrash(node: Node): bool = {
    val ns = rethStates.get(node)
    all {
      ns.mem.phase != Offline,
      rethStates' = rethStates.set(node, initRethNode),
      requestHistory' = requestHistory.set(node, []),
      engineLastTraceEntry' = Some({ node: node, entry: EngineFaultMsg(Crash) }),
      nextId' = nextId,
    }
  }

  // --- rethRestart ---
  // Node restarted: preserve disk state, reset mem, clear request history.
  // Chain and head survive on disk. Validated blocks and pending builds lost.
  action rethRestart(node: Node): bool = {
    val ns = rethStates.get(node)
    all {
      ns.mem.phase != Offline,
      rethStates' = rethStates.set(node, { disk: ns.disk, mem: readyRethMem }),
      requestHistory' = requestHistory.set(node, []),
      engineLastTraceEntry' = Some({ node: node, entry: EngineFaultMsg(Restart) }),
      nextId' = nextId,
    }
  }

  // ===========================================================================
  // INITIALIZATION
  // ===========================================================================

  action init = {
    val nodes = NODES.toSet()
    all {
      rethStates' = nodes.mapBy(n => initRethNode),
      requestHistory' = nodes.mapBy(n => []),
      engineLastTraceEntry' = None,
      nextId' = 1,
    }
  }

  // ===========================================================================
  // STEP: nondeterministic test driver
  // ===========================================================================
  //
  // For standalone testing: a nondeterministic caller picks which Engine API
  // call to make. The generator responds. This validates the contract
  // independently of Emerald.

  action step = {
    nondet node = NODES.toSet().oneOf()
    val ns = rethStates.get(node)
    any {
      // Start Reth
      rethStart(node),
      // newPayload: Test driver explores ALL possible heights and validity scenarios.
      //
      // NOTE: This is deliberately MORE GENERAL than real usage in Emerald composition.
      // In emerald_with_both_generators.qnt, newPayload is only called at heights
      // driven by consensus progression (sequential: 1 → 2 → 3...).
      //
      // Here we test the Engine API contract exhaustively by:
      // 1. Picking random heights (1 to MAX_HEIGHT) - tests out-of-order validation
      // 2. Nondeterministically choosing valid/invalid parentHash - tests both paths
      //
      // This validates that the contract properties hold even under adversarial or
      // nonsensical calling patterns, proving Reth's validation logic is correct
      // regardless of how the caller behaves.
      nondet height = 1.to(MAX_HEIGHT).oneOf()  // Explore all heights (not just sequential)
      nondet validParent = Set(true, false).oneOf()  // Test both valid and invalid blocks
      val parentHash = if (ns.disk.chain.has(height - 1))
        ns.disk.chain.get(height - 1).hash
      else
        -1  // Parent doesn't exist in chain
      val block: Block = {
        height: height,
        hash: blockHash(height, nextId + 100),
        // If validParent=true and parent exists: use correct parentHash (valid block)
        // If validParent=false or parent missing: use -1 (invalid block)
        parentHash: if (validParent and parentHash != -1) parentHash else -1,
      }
      respondNewPayload(node, block),
      // forkchoiceUpdated: build request (always builds on current head)
      respondBuildRequest(node),
      // forkchoiceUpdated: head update (test driver respects Phase 1 constraints)
      //
      // NOTE: This explores forward head progression only, respecting the Phase 1
      // no-reorg constraint. In emerald_with_both_generators.qnt, head updates
      // are driven by consensus Decided events (sequential: height 1 → 2 → 3...).
      //
      // Here we test:
      // 1. Idempotent updates (same height/hash)
      // 2. Forward progression (higher heights)
      // 3. Skip-ahead scenarios (jumping multiple heights)
      //
      // The range starts at current headHeight (not 1) because respondHeadUpdate's
      // guard blocks backward movement: headHeight > ns.disk.headHeight.
      nondet updateHeight = ns.disk.headHeight.to(MAX_HEIGHT).oneOf()  // Forward only
      // Lookup the hash for the chosen height:
      // - If same as current height: use current head (tests idempotency)
      // - If block exists at updateHeight: use its hash (tests forward progress)
      // - If block doesn't exist: use -1 (filtered by guard below)
      val updateHash = if (updateHeight == ns.disk.headHeight) ns.disk.head
        else if (ns.disk.chain.has(updateHeight)) ns.disk.chain.get(updateHeight).hash
        else -1
      // Guard: only call respondHeadUpdate if we found a valid block
      // Filters out cases where updateHeight has no block in chain yet
      all { updateHash != -1, respondHeadUpdate(node, updateHash, updateHeight) },
      // getPayload
      respondGetPayload(node),
      // Faults
      rethCrash(node),
      rethRestart(node),
    }
  }

  // ===========================================================================
  // CONTRACT PROPERTIES (checked as invariants)
  // ===========================================================================

  // Build per-node EngineState for contract checking.
  // Each node has its own Reth instance, so disk state and request history
  // are checked independently per node.
  pure def engineStateForNode(
    ns: RethNodeState,
    hist: List[EngineRequest],
    node: Node
  ): EngineState = {
    disk: {
      chain: ns.disk.chain,
      head: ns.disk.head,
      headHeight: ns.disk.headHeight,
    },
    mem: {
      request_history: Map(node -> hist),
    },
  }

  // Full contract checked per-node
  val contractInv = NODES.toSet().forall(n =>
    engineContract(engineStateForNode(rethStates.get(n), requestHistory.get(n), n))
  )
}
