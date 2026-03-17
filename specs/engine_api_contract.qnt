// -*- mode: Bluespec; -*-

// =============================================================================
// Engine API Contract (DRAFT)
// =============================================================================
//
// Behavioral contract for Reth's Engine API boundary (Emerald↔Reth).
// Written by the Emerald team as a proof of concept for the contract pattern.
// Models what the Reth team would publish for Emerald to consume.
//
// This contract defines:
//   1. The request/response types for the Engine API
//   2. Server guarantees (Reth → responses): response correctness
//   3. Client guarantees (Emerald → requests): request ordering
//   4. Phase 1 constraints
//
// Key structural difference from Channel API:
//   - Channel API: Malachite sends requests to Emerald (Emerald responds)
//   - Engine API: Emerald sends requests to Reth (Reth responds)
//   - Generator models Reth producing responses to Emerald's requests
//
// Phase 1 scope: 3 core methods
//   - engine_forkchoiceUpdatedV3(head, attrs?)
//   - engine_newPayloadV4(payload, ...)
//   - engine_getPayloadV4(payloadId)
//
// State follows the disk/mem convention from faults.qnt:
//   - Disk: chain, head (persist through restart)
//   - Mem: request_history (cleared on fault, current session only)
//
// Source: Reth's Engine API implementation + Ethereum Engine API spec.
//
// =============================================================================
// WHY PER-NODE CONTRACT CHECKING?
// =============================================================================
//
// Unlike the Channel API (which checks properties globally across all nodes),
// the Engine API contract is checked INDEPENDENTLY for each node's Reth instance.
// This reflects the fundamental architectural difference between the two APIs:
//
// Channel API (Malachite ↔ Emerald):
//   - Models a DISTRIBUTED CONSENSUS PROTOCOL
//   - Nodes coordinate via Malachite to reach agreement
//   - Properties check CROSS-NODE consistency (e.g., agreement: all nodes
//     deciding the same proposal at a given height)
//   - Shared global state: decisions map, proposal support (quorum tracking)
//   - Contract must see ALL nodes' histories to verify consensus safety
//
// Engine API (Emerald ↔ Reth):
//   - Models a LOCAL RPC INTERFACE between one Emerald and one Reth
//   - Each node runs its own INDEPENDENT Reth execution engine
//   - No coordination at the Engine API boundary (Reth instances don't
//     communicate through this API)
//   - Properties check SINGLE-INSTANCE correctness (e.g., validationStability:
//     this Reth instance never changes Valid → Invalid)
//   - Per-node state: each Reth has its own chain, head, validation cache
//   - Each Emerald only observes its own Reth instance
//
// What about Reth's P2P layer (syncing)?
//   - When Phase 2 adds SYNCING state, Reth nodes will sync blocks from each
//     other via p2p (Reth1 ↔ Reth2).
//   - This is an IMPLEMENTATION DETAIL of how Reth gets block data, not part
//     of the Engine API boundary.
//   - The contract specifies WHAT Emerald observes (SYNCING → VALID transitions),
//     not HOW Reth gets blocks (p2p, snapshots, database).
//   - Contract properties remain per-node: "If my Reth transitions SYNCING → VALID
//     for block B, it never later says INVALID" (single-instance consistency).
//   - The generator may model cross-node sync actions to produce realistic traces,
//     but contract properties still check each Reth instance independently.
//
// Why this matters:
//   - Correct abstraction: Contract models the observable API boundary, not
//     internal implementation (p2p layer is internal to Reth).
//   - Composability: Emerald can reason about its local Reth without needing
//     to know about other nodes' Reth instances.
//   - Testability: Properties can be verified against a single Reth instance
//     in isolation (useful for Reth developers).
//
// Generator checking strategy:
//   The generator checks: NODES.toSet().forall(n => componentGuarantees(stateForNode(n)))
//   This means: "Every Reth instance independently satisfies the server-side contract."
//   Compare to Channel API: componentGuarantees(globalState) checks cross-node properties
//   like agreement over all nodes simultaneously.

module engine_api_contract {
  import basicSpells.* from "spells/basicSpells"

  // ===========================================================================
  // TYPES (abstract — no real EVM types, protocol-level structure)
  // ===========================================================================

  // Shared with channel_api_contract (same structural types)
  type Node = str
  type Height = int

  type BlockHash = int
  type PayloadId = int

  type Block = {
    height: Height,
    hash: BlockHash,
    parentHash: BlockHash,
  }

  type PayloadStatusCode =
    | Valid
    | Invalid

  // Request-response pairs logged at the Engine API boundary.
  // Each variant captures both the request parameters and the response.
  // Response fields are prefixed with "resp" to distinguish them from
  // request fields.
  type EngineRequest =
    | ReqNewPayload({ block: Block, respStatus: PayloadStatusCode })
    | ReqForkchoiceUpdated({
        headHash: BlockHash,
        building: bool,
        respStatus: PayloadStatusCode,
        respPayloadId: Option[PayloadId],
      })
    | ReqGetPayload({ payloadId: PayloadId, respBlock: Block })

  // ===========================================================================
  // OBSERVABLE STATE
  // ===========================================================================
  //
  // State that any conforming generator must maintain for contract properties
  // to be checkable. Follows the disk/mem convention from faults.qnt.

  type EngineStateDisk = {
    // Canonical chain: height → block (persisted by Reth)
    chain: Height -> Block,
    // Current canonical head hash
    head: BlockHash,
    // Current head height
    headHeight: Height,
  }

  type EngineStateMem = {
    // Per-node: request history for the current session.
    // Cleared on fault — contains only current-session requests.
    request_history: Node -> List[EngineRequest],
  }

  type EngineState = {
    disk: EngineStateDisk,
    mem: EngineStateMem,
  }

  // ===========================================================================
  // SERVER GUARANTEES: RESPONSE CORRECTNESS
  // ===========================================================================

  /// @guarantor component
  /// @category safety
  /// @scope per-node
  // validationStability: Once newPayload(block) returns Valid, subsequent
  // calls for the same block hash never return Invalid.
  //
  // Reth persists validation results. Once a block is accepted as valid,
  // it remains valid. This lets Emerald cache validation status safely.
  //
  // Source: Engine API spec — payload status is deterministic for a given
  // payload. Valid payloads do not become Invalid.
  pure def validationStability(s: EngineState): bool =
    s.mem.request_history.keys().forall(node =>
      val hist = s.mem.request_history.get(node)
      // No subsequent newPayload for a validated hash returns Invalid
      hist.foldl({ validated: Set(), ok: true }, (acc, ex) =>
        if (not(acc.ok)) acc
        else match ex {
          | ReqNewPayload(np) =>
              match np.respStatus {
                | Valid =>
                    { ...acc, validated: acc.validated.union(Set(np.block.hash)) }
                | Invalid =>
                    { ...acc, ok: not(acc.validated.contains(np.block.hash)) }
              }
          | _ => acc
        }
      ).ok
    )

  /// @guarantor component
  /// @category safety
  /// @scope per-node
  // buildIntegrity: getPayload(id) returns a block whose parentHash matches
  // the head from the forkchoiceUpdated that produced that payloadId.
  //
  // When Emerald calls forkchoiceUpdated with building=true, Reth starts
  // building on top of the current head. The resulting block must extend
  // that head.
  //
  // Source: Engine API spec — getPayload returns the execution payload
  // built on the state identified by the prior forkchoiceUpdated.
  pure def buildIntegrity(s: EngineState): bool =
    s.mem.request_history.keys().forall(node =>
      val hist = s.mem.request_history.get(node)
      // Single pass: collect payloadId → headHash from FCU, check getPayload against it.
      // Works because forkchoiceUpdated always precedes its getPayload in the history.
      hist.foldl({ buildHeads: Map(), ok: true }, (acc, ex) =>
        if (not(acc.ok)) acc
        else match ex {
          | ReqForkchoiceUpdated(fcu) =>
              if (fcu.building) match fcu.respPayloadId {
                | Some(pid) =>
                    { ...acc, buildHeads: acc.buildHeads.put(pid, fcu.headHash) }
                | None => acc
              }
              else acc
          | ReqGetPayload(gp) =>
              { ...acc, ok: acc.buildHeads.has(gp.payloadId) and
                          gp.respBlock.parentHash == acc.buildHeads.get(gp.payloadId) }
          | _ => acc
        }
      ).ok
    )

  /// @guarantor component
  /// @category safety
  /// @scope per-node
  // headProgression: After forkchoiceUpdated(hash) returns Valid, the
  // canonical head is updated to that hash.
  //
  // A successful forkchoiceUpdated sets the execution head. This is the
  // primary mechanism for finalizing blocks.
  //
  // Checked globally (not per-node) since Reth has a single canonical chain.
  //
  // Source: Engine API spec — forkchoiceUpdated updates the fork choice state.
  pure def headProgression(s: EngineState): bool =
    s.mem.request_history.keys().forall(node =>
      val hist = s.mem.request_history.get(node)
      hist.length() == 0 or {
        // The last successful forkchoiceUpdated determines the head.
        // Check that the global head matches the last valid FCU.
        // Note: foldl is used as "find last match" — Quint would benefit from findLast.
        val lastValidFCU: Option[BlockHash] = hist.foldl(None, (acc, ex) =>
          match ex {
            | ReqForkchoiceUpdated(fcu) =>
                match fcu.respStatus {
                  | Valid => Some(fcu.headHash)
                  | _ => acc
                }
            | _ => acc
          }
        )
        match lastValidFCU {
          | Some(h) => s.disk.head == h
          | None => true
        }
      }
    )

  /// @guarantor component
  /// @category safety
  /// @scope per-node
  // validBeforeHead: forkchoiceUpdated(hash) returning Valid requires that
  // the block was previously known — either via newPayload returning Valid,
  // getPayload returning it (built by Reth), or already in the chain.
  //
  // Source: Engine API spec — INVALID_FORKCHOICE_STATE if the head
  // references an unknown or invalid payload.
  pure def validBeforeHead(s: EngineState): bool =
    s.mem.request_history.keys().forall(node =>
      val hist = s.mem.request_history.get(node)
      // Track hashes known via newPayload(Valid) + getPayload + chain
      val chainHashes: Set[BlockHash] = s.disk.chain.keys().fold(Set(), (acc, h) =>
        acc.union(Set(s.disk.chain.get(h).hash))
      )
      hist.foldl({ validated: chainHashes, ok: true }, (acc, ex) =>
        if (not(acc.ok)) acc
        else match ex {
          | ReqNewPayload(np) =>
              match np.respStatus {
                | Valid => { ...acc, validated: acc.validated.union(Set(np.block.hash)) }
                | _ => acc
              }
          | ReqGetPayload(gp) =>
              // Built by Reth — implicitly valid
              { ...acc, validated: acc.validated.union(Set(gp.respBlock.hash)) }
          | ReqForkchoiceUpdated(fcu) =>
              match fcu.respStatus {
                | Valid => { ...acc, ok: acc.validated.contains(fcu.headHash) }
                | _ => acc
              }
          | _ => acc
        }
      ).ok
    )

  /// @guarantor component
  /// @category safety
  /// @scope per-node
  // chainContinuity: Blocks in the canonical chain form a linked list
  // via parentHash.
  //
  // Each block at height h > 1 must have parentHash equal to the hash
  // of the block at height h-1.
  //
  // Source: Ethereum block structure — blocks form a chain via parentHash.
  pure def chainContinuity(s: EngineState): bool =
    s.disk.chain.keys().forall(h =>
      h == 0 or {
        s.disk.chain.has(h - 1) and
        s.disk.chain.get(h).parentHash == s.disk.chain.get(h - 1).hash
      }
    )

  // ===========================================================================
  // PHASE 1 CONSTRAINTS (to be relaxed in Phase 2)
  // ===========================================================================

  /// @guarantor component
  /// @category phase1
  /// @scope per-node
  // headMonotonic: The canonical head height never decreases (no reorgs).
  //
  // Phase 1 constraint: Once the head advances to height h, it never moves
  // back to a lower height. This simplifies initial modeling by excluding
  // reorg scenarios.
  //
  // In Phase 1:
  //   - forkchoiceUpdated can only move head forward or stay at same height
  //   - Idempotent updates (same height/hash) are allowed for retry handling
  //
  // Phase 2 will relax this to allow reorgs:
  //   - Head can move to different blocks at same or lower heights
  //   - Models Ethereum's fork choice with chain reorganizations
  //
  // NOTE: This is NOT a fundamental Reth guarantee — real Reth supports reorgs.
  // This property is enforced by the generator's state machine guards, not by
  // Reth's implementation.
  pure def headMonotonic(s: EngineState): bool =
    s.mem.request_history.keys().forall(node =>
      val hist = s.mem.request_history.get(node)
      // Track head height progression through successful forkchoiceUpdated calls
      hist.foldl({ maxHeight: 0, ok: true }, (acc, ex) =>
        if (not(acc.ok)) acc
        else match ex {
          | ReqForkchoiceUpdated(fcu) =>
              match fcu.respStatus {
                | Valid =>
                    // After this FCU, head is at the height of the referenced block
                    val newHeight = s.disk.chain.keys().fold(-1, (maxH, h) =>
                      if (s.disk.chain.get(h).hash == fcu.headHash and h > maxH) h else maxH
                    )
                    { maxHeight: if (newHeight > acc.maxHeight) newHeight else acc.maxHeight,
                      ok: newHeight >= acc.maxHeight }
                | _ => acc
              }
          | _ => acc
        }
      ).ok
    )

  // Combined response correctness properties
  pure def responseCorrectness(s: EngineState): bool = and {
    validationStability(s),
    buildIntegrity(s),
    headProgression(s),
    validBeforeHead(s),
    chainContinuity(s),
  }

  // Phase 1 specific constraints
  pure def phase1Constraints(s: EngineState): bool = and {
    headMonotonic(s),
  }

  // ===========================================================================
  // CLIENT GUARANTEES: REQUEST ORDERING
  // ===========================================================================

  /// @guarantor application
  /// @category ordering
  /// @scope per-node
  // buildLifecycle: getPayload(id) must follow a forkchoiceUpdated that
  // returned that id. Each payloadId is consumed at most once.
  //
  // Source: Engine API spec — payloadId is a one-time-use token.
  pure def buildLifecycle(s: EngineState): bool =
    s.mem.request_history.keys().forall(node =>
      val hist = s.mem.request_history.get(node)
      hist.foldl({ available: Set(), ok: true }, (acc, ex) =>
        if (not(acc.ok)) acc
        else match ex {
          | ReqForkchoiceUpdated(fcu) =>
              if (fcu.building) match fcu.respPayloadId {
                | Some(pid) =>
                    { ...acc, available: acc.available.union(Set(pid)) }
                | None => acc
              }
              else acc
          | ReqGetPayload(gp) =>
              { ok: acc.available.contains(gp.payloadId),
                available: acc.available.exclude(Set(gp.payloadId)) }
          | _ => acc
        }
      ).ok
    )

  // Combined request ordering properties
  pure def requestOrdering(s: EngineState): bool = and {
    buildLifecycle(s),
  }

  // ===========================================================================
  // FULL CONTRACT
  // ===========================================================================

  // Server (Reth) guarantees: response correctness + phase 1 constraints.
  pure def componentGuarantees(s: EngineState): bool = and {
    responseCorrectness(s),
    phase1Constraints(s),
  }

  // Client (Emerald) guarantees: request ordering.
  pure def applicationGuarantees(s: EngineState): bool = and {
    requestOrdering(s),
  }

  // Full contract: both sides.
  pure def engineContract(s: EngineState): bool = and {
    componentGuarantees(s),
    applicationGuarantees(s),
  }
}
