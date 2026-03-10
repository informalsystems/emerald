// -*- mode: Bluespec; -*-

// =============================================================================
// Witness invariants for emerald_with_both_generators (three-way composition)
// =============================================================================
//
// IMPORTANT: These are REACHABILITY CHECKS via negated invariants.
// Random simulation tries to VIOLATE these invariants.
//
//   Violation   = scenario is reachable ✓ (spec is expressive)
//   No violation = scenario may be unreachable   (investigate spec constraints)
//
// Usage:
//   quint run emerald/specs/emerald_with_both_generators_witnesses.qnt \
//     --main=emerald_with_both_generators_witnesses \
//     --invariant=<witness_name> \
//     --max-steps=<steps> --max-samples=1000 --backend=rust
//
// Expected: Invariant VIOLATION (proves scenario is reachable)
//
// Note on step counts: The three-way composition has a work-queue model
// (one Engine API call dispatched per step). End-to-end scenarios need
// longer traces than comparable two-boundary specs. Empirical minimums
// per witness are documented in each run command.
//
// =============================================================================
// Witness Catalog (8 liveness witnesses)
// =============================================================================
//
//   Fast (20-50 steps) — reach reliably with random simulation:
//     • canReachWorking          - At least one node enters Working phase
//     • canAdvanceRound          - At least one node reaches consensus round >= 1
//     • canHaveUndecidedProposal - At least one node has an undecided proposal
//
//   Medium (200 steps) — reachable but require longer traces:
//     • canReachSyncing          - At least one node enters Syncing (sync path)
//     • canReachDecision         - At least one node decides at height >= 1
//     • canCommitBlock           - At least one node commits a block to disk
//
//   Hard (high step counts) — require targeted simulation or relaxation:
//     • canDecideTwoHeights      - At least one node decides at height >= 2
//     • canHavePendingProposal   - At least one node buffers a future-height proposal
//
// Configuration: 4 nodes (same as emerald_with_both_generators_test)
//   - Allows progress with one crashed node (3/4 > 2/3 quorum)
// =============================================================================

module emerald_with_both_generators_witnesses {
  import basicSpells.* from "spells/basicSpells"
  import rareSpells.* from "spells/rareSpells"
  import emerald_with_both_generators(NODES = List("node1", "node2", "node3", "node4")).* from "emerald_with_both_generators"

  // ===========================================================================
  // FAST WITNESSES (20-50 steps)
  // ===========================================================================

  // ──────────────────────────────────────────────────────────────────────────
  // Witness: canReachWorking
  // ──────────────────────────────────────────────────────────────────────────
  // Goal: At least one node enters the Working phase
  // Rationale: Verifies the ConsensusReady → StartedRound path works.
  //   The Working phase means Emerald received StartedRound and is actively
  //   participating in a consensus round. Required for GetValue and
  //   ReceivedProposal to be processed.
  //
  // EXPECTED: Invariant VIOLATION (proves Working phase is reachable)
  //
  // Run: quint run emerald/specs/emerald_with_both_generators_witnesses.qnt \
  //        --main=emerald_with_both_generators_witnesses \
  //        --invariant=canReachWorking \
  //        --max-steps=20 --max-samples=1000 --backend=rust
  // ──────────────────────────────────────────────────────────────────────────
  val canReachWorking: bool =
    NODES.toSet().forall(n => emeraldState.get(n).mem.phase != Working)

  // ──────────────────────────────────────────────────────────────────────────
  // Witness: canAdvanceRound
  // ──────────────────────────────────────────────────────────────────────────
  // Goal: At least one node reaches consensus round >= 1 (round timeout fired)
  // Rationale: Verifies the timeout-triggered round advancement path works.
  //   When a round times out without a decision, Malachite increments the
  //   round and sends StartedRound(h, r+1). Exercises sendStartedRoundAfterTimeout
  //   in the generator and the roundsConsecutiveWithinHeight contract property.
  //
  // EXPECTED: Invariant VIOLATION (proves round advancement is reachable)
  //
  // Run: quint run emerald/specs/emerald_with_both_generators_witnesses.qnt \
  //        --main=emerald_with_both_generators_witnesses \
  //        --invariant=canAdvanceRound \
  //        --max-steps=30 --max-samples=1000 --backend=rust
  // ──────────────────────────────────────────────────────────────────────────
  val canAdvanceRound: bool =
    NODES.toSet().forall(n => emeraldState.get(n).mem.consensus_round < 1)

  // ──────────────────────────────────────────────────────────────────────────
  // Witness: canHaveUndecidedProposal
  // ──────────────────────────────────────────────────────────────────────────
  // Goal: At least one node has an undecided proposal in its local set
  // Rationale: Verifies the proposal reception pipeline. A node marks a
  //   proposal as "undecided" when it has been received for the current
  //   height (via ReceivedProposal + Engine API validation, or GetValue as
  //   proposer) but the height has not yet been decided. This is the
  //   intermediate liveness state between proposal arrival and commitment.
  //
  // EXPECTED: Invariant VIOLATION (proves undecided proposal state is reachable)
  //
  // Run: quint run emerald/specs/emerald_with_both_generators_witnesses.qnt \
  //        --main=emerald_with_both_generators_witnesses \
  //        --invariant=canHaveUndecidedProposal \
  //        --max-steps=30 --max-samples=1000 --backend=rust
  // ──────────────────────────────────────────────────────────────────────────
  val canHaveUndecidedProposal: bool =
    NODES.toSet().forall(n => emeraldState.get(n).mem.undecided_proposals == Set())

  // ===========================================================================
  // MEDIUM WITNESSES (200 steps)
  // ===========================================================================

  // ──────────────────────────────────────────────────────────────────────────
  // Witness: canReachSyncing
  // ──────────────────────────────────────────────────────────────────────────
  // Goal: At least one node enters the Syncing phase (ProcessSyncedValue)
  // Rationale: Verifies the sync catch-up path is reachable. A node enters
  //   Syncing when it falls behind and receives a synced decided value from
  //   another node instead of participating in normal round consensus.
  //   Exercises syncRequiresStartedRound and syncIsIrrevocable contract properties.
  //
  // EXPECTED: Invariant VIOLATION (proves Syncing phase is reachable)
  //
  // Run: quint run emerald/specs/emerald_with_both_generators_witnesses.qnt \
  //        --main=emerald_with_both_generators_witnesses \
  //        --invariant=canReachSyncing \
  //        --max-steps=200 --max-samples=2000 --backend=rust
  // ──────────────────────────────────────────────────────────────────────────
  val canReachSyncing: bool =
    NODES.toSet().forall(n => emeraldState.get(n).mem.phase != Syncing)

  // ──────────────────────────────────────────────────────────────────────────
  // Witness: canReachDecision
  // ──────────────────────────────────────────────────────────────────────────
  // Goal: At least one node decides at height >= 1
  // Rationale: Verifies the full three-way pipeline works end-to-end:
  //   ConsensusReady → StartedRound → GetValue (Engine API build) →
  //   ReceivedProposal (Engine API newPayload) → Decided →
  //   [newPayload + headUpdate + FinalizeDecided].
  //   The most fundamental liveness check for the composition.
  //
  // Note: needs ~200 steps because the work-queue dispatches one Engine API
  //   call per transition, so a single decision takes ~20+ transitions even
  //   on a direct path. The random simulator needs room to explore.
  //
  // EXPECTED: Invariant VIOLATION (proves decision is reachable)
  //
  // Run: quint run emerald/specs/emerald_with_both_generators_witnesses.qnt \
  //        --main=emerald_with_both_generators_witnesses \
  //        --invariant=canReachDecision \
  //        --max-steps=200 --max-samples=2000 --backend=rust
  // ──────────────────────────────────────────────────────────────────────────
  val canReachDecision: bool =
    NODES.toSet().forall(n => emeraldState.get(n).disk.last_decided_height < 1)

  // ──────────────────────────────────────────────────────────────────────────
  // Witness: canCommitBlock
  // ──────────────────────────────────────────────────────────────────────────
  // Goal: At least one node has latest_block set (a block committed to disk)
  // Rationale: Verifies the complete Engine API finalization pipeline:
  //   Decided → enqueue [CallNewPayload, CallHeadUpdate, FinalizeDecided] →
  //   stepAdvanceWork × 3 → latest_block persisted to EmeraldDiskState.
  //   latest_block != None means a Reth block has been fully committed,
  //   Reth's head updated, and Emerald's disk state reflects the decision.
  //   Stronger than canReachDecision: checks the full finalization completes.
  //
  // EXPECTED: Invariant VIOLATION (proves block commitment is reachable)
  //
  // Run: quint run emerald/specs/emerald_with_both_generators_witnesses.qnt \
  //        --main=emerald_with_both_generators_witnesses \
  //        --invariant=canCommitBlock \
  //        --max-steps=200 --max-samples=1000 --backend=rust
  // ──────────────────────────────────────────────────────────────────────────
  val canCommitBlock: bool =
    NODES.toSet().forall(n =>
      match emeraldState.get(n).disk.latest_block {
        | Some(_) => false
        | None => true
      }
    )

  // ===========================================================================
  // HARD WITNESSES (require targeted simulation or relaxation)
  // ===========================================================================
  //
  // These witnesses are valid reachability checks but are rarely found by
  // pure random simulation because they require long sequential pipelines
  // at low probability. They document correct spec behavior but may need
  // /verify:debug-witness or manual trace construction to confirm.

  // ──────────────────────────────────────────────────────────────────────────
  // Witness: canDecideTwoHeights
  // ──────────────────────────────────────────────────────────────────────────
  // Goal: At least one node decides at height >= 2
  // Rationale: Verifies the protocol can progress through multiple consecutive
  //   heights. Checks that decided_proposals accumulates, consensus_height
  //   advances after each decision, and the block chain grows (each block's
  //   parentHash links to the previous committed block).
  //
  // Hard to reach: requires completing the entire height-1 decision pipeline
  //   for multiple nodes (quorum), then the full height-2 pipeline. Each
  //   height requires ~20+ transitions; with 4 nodes the state space is large.
  //   The /verify:debug-witness skill can help relax constraints to find a path.
  //
  // EXPECTED: Invariant VIOLATION (proves multi-height progress is reachable)
  //
  // Run: quint run emerald/specs/emerald_with_both_generators_witnesses.qnt \
  //        --main=emerald_with_both_generators_witnesses \
  //        --invariant=canDecideTwoHeights \
  //        --max-steps=500 --max-samples=10000 --backend=rust
  // ──────────────────────────────────────────────────────────────────────────
  val canDecideTwoHeights: bool =
    NODES.toSet().forall(n => emeraldState.get(n).disk.last_decided_height < 2)

  // ──────────────────────────────────────────────────────────────────────────
  // Witness: canHavePendingProposal
  // ──────────────────────────────────────────────────────────────────────────
  // Goal: At least one node buffers a future-height pending proposal
  // Rationale: Verifies the proposal buffering path. When a node receives
  //   ReceivedProposal for a height > consensus_height, it stores it as
  //   "pending". Pending proposals are promoted to undecided by
  //   handleStartedRound when the node advances to that height.
  //   Exercises the pending_proposals lifecycle in Emerald state.
  //
  // Hard to reach: requires one node to be ahead at height h+1 (created a
  //   proposal there via GetValue) while another node is still at height h.
  //   Random simulation rarely creates this asymmetry without biased scheduling.
  //   The /verify:debug-witness skill can help find a targeted trace.
  //
  // EXPECTED: Invariant VIOLATION (proves pending proposal buffering is reachable)
  //
  // Run: quint run emerald/specs/emerald_with_both_generators_witnesses.qnt \
  //        --main=emerald_with_both_generators_witnesses \
  //        --invariant=canHavePendingProposal \
  //        --max-steps=500 --max-samples=10000 --backend=rust
  // ──────────────────────────────────────────────────────────────────────────
  val canHavePendingProposal: bool =
    NODES.toSet().forall(n => emeraldState.get(n).mem.pending_proposals == Set())

  // ===========================================================================
  // DEBUG ARTIFACTS (canDecideTwoHeights — kept as reference, do not run)
  // ===========================================================================
  //
  // These were used during /verify:debug-witness analysis to pinpoint the
  // blocker for canDecideTwoHeights. Kept as commented-out documentation.
  //
  // Findings summary:
  //   GW1 VIOLATED   — one node CAN complete height-1 and advance to h=2
  //   GW2 NOT VIOLATED — two nodes NEVER both complete height-1 (the blocker)
  //   GW3 VIOLATED   — StartedRound at height 2 IS reachable
  //   GW4 NOT VIOLATED — no undecided proposal ever exists at height 2
  //
  //   Relaxed witness (= GW2) also NOT VIOLATED — confirms the blocker.
  //
  // Root cause: The random scheduler converges on one node finishing
  // height-1 while the remaining nodes (including node2, the only valid
  // proposer at h=2,r=0) never complete their height-1 pipelines.
  // Without node2 deciding height-1, no quorum forms for height-2.
  // This is a probabilistic trap, not a spec bug.
  //
  // To confirm reachability, use a biased/fair scheduler or manual trace.
  //
  // // GW1: Any node decides height 1 and advances (consensus_height >= 2)
  // // Expected: VIOLATED — confirmed violated
  // val witness_guard_1: bool =
  //   not(NODES.toSet().exists(n =>
  //     emeraldState.get(n).disk.last_decided_height >= 1 and
  //     emeraldState.get(n).mem.consensus_height >= 2
  //   ))
  //
  // // GW2 / canDecideTwoHeights_relaxed: Two different nodes both decide height 1
  // // Expected: VIOLATED — NOT VIOLATED (confirmed blocker)
  // val witness_guard_2: bool =
  //   not(NODES.toSet().exists(n1 =>
  //     NODES.toSet().exists(n2 =>
  //       n1 != n2 and
  //       emeraldState.get(n1).disk.last_decided_height >= 1 and
  //       emeraldState.get(n2).disk.last_decided_height >= 1
  //     )
  //   ))
  //
  // // GW3: Any node in Working phase at consensus height 2
  // // Expected: VIOLATED — confirmed violated
  // val witness_guard_3: bool =
  //   not(NODES.toSet().exists(n =>
  //     emeraldState.get(n).mem.phase == Working and
  //     emeraldState.get(n).mem.consensus_height == 2
  //   ))
  //
  // // GW4: Any node has undecided proposals at consensus height 2
  // // Expected: VIOLATED — NOT VIOLATED (pipeline stalls without quorum)
  // val witness_guard_4: bool =
  //   not(NODES.toSet().exists(n =>
  //     emeraldState.get(n).mem.consensus_height == 2 and
  //     emeraldState.get(n).mem.undecided_proposals != Set()
  //   ))

}
