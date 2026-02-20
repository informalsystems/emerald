// -*- mode: Bluespec; -*-

// =============================================================================
// Channel API Contract (DRAFT)
// =============================================================================
//
// Behavioral contract for Malachite's Channel API.
// Published by the Malachite team. Consumed by applications (e.g., Emerald).
//
// This contract defines:
//   1. The message types exchanged through the Channel API
//   2. Safety guarantees (agreement, validity)
//   3. Ordering guarantees (declarative properties over event histories)
//   4. Assumptions (what the application must do for guarantees to hold)
//
// Both safety and ordering properties are checked as invariants on any
// conforming generator (e.g., channel_api_generator.qnt). The generator
// enforces ordering through its state machine structure; these declarative
// properties make the ordering auditable and verifiable independently.
//
// Fault handling: Faults (crash/restart) are node-level, not API-specific.
// After a fault, the node's event history is cleared (it's ephemeral/mem
// state), while proposals and decisions survive on disk. This means contract
// properties only need to reason about the current session's events — no
// EvFault variant or session-reset logic needed.
//
// State follows the disk/mem convention from faults.qnt:
//   - Disk: all_proposals, decisions (survive crash/restart)
//   - Mem: event_history (cleared on fault, contains only current session)
//
// Source: Malachite's app-channel crate (crates/app-channel/src/msgs.rs)
// and the consensus Quint spec (specs/consensus/quint/).

module channel_api_contract {
  import basicSpells.* from "spells/basicSpells"

  // ===========================================================================
  // TYPES (derived from AppMsg in msgs.rs)
  // ===========================================================================

  type Node = str
  type Height = int
  type Round = int
  type Payload = int

  type Proposal = {
    height: Height,
    round: Round,
    proposer: Node,
    payload: Payload,
  }

  // Messages from Malachite consensus to the application, observable at the
  // Channel API boundary. This is the subset relevant to Emerald.
  //
  // Not modeled here (future work):
  //   - ExtendVote / VerifyVoteExtension (vote extension lifecycle)
  //   - RestreamProposal (re-broadcast)
  //   - GetHistoryMinHeight (sync protocol detail)
  //   - ReceivedProposalPart (modeled as ReceivedProposal for simplicity)
  //
  // Faults (crash/restart) are NOT part of the Channel API event type.
  // They are node-level events handled by clearing event histories.
  // See faults.qnt for the FaultEvent type.
  type ChannelEvent =
    | EvConsensusReady
    | EvStartedRound({ height: Height, round: Round, proposer: Node })
    | EvGetValue({ height: Height, round: Round })
    | EvReceivedProposal({ proposal: Proposal })
    | EvDecided({ proposal: Proposal })
    | EvProcessSyncedValue({ proposal: Proposal })
    | EvGetDecidedValue({ height: Height })

  // ===========================================================================
  // EVENT HELPERS
  // ===========================================================================

  /// Extract the height from an event, if it has one.
  pure def eventHeight(e: ChannelEvent): Option[Height] = match e {
    | EvConsensusReady => None
    | EvStartedRound(r) => Some(r.height)
    | EvGetValue(r) => Some(r.height)
    | EvReceivedProposal(r) => Some(r.proposal.height)
    | EvDecided(r) => Some(r.proposal.height)
    | EvProcessSyncedValue(r) => Some(r.proposal.height)
    | EvGetDecidedValue(r) => Some(r.height)
  }

  /// Whether an event is a consensus progress event (vs. a query/delivery event).
  /// Progress events follow monotonic height ordering and are subject to
  /// heightMonotonic and decidedIsFinal.
  /// Excluded events:
  ///   - EvConsensusReady: one-time initialization, not height progress
  ///   - EvGetDecidedValue: intentionally queries past heights
  ///   - EvReceivedProposal: can arrive for future heights (buffered as pending)
  pure def isProgressEvent(e: ChannelEvent): bool = match e {
    | EvConsensusReady => false
    | EvGetDecidedValue(_) => false
    | EvReceivedProposal(_) => false
    | _ => true
  }

  // ===========================================================================
  // OBSERVABLE STATE
  // ===========================================================================
  //
  // State that any conforming generator must maintain for contract properties
  // to be checkable. This is the "public interface" of the generator's state.
  //
  // Follows the disk/mem convention from faults.qnt:
  //   - Disk state survives restarts (proposals, decisions)
  //   - Mem state is cleared on fault (event histories — current session only)
  //
  // Design: event_history is the primary observable — most properties are
  // checked over it. decisions and all_proposals are kept as separate fields
  // for efficient lookup by validity and syncedValueIsDecided.

  type ChannelStateDisk = {
    // All proposals that have been made (via GetValue responses)
    all_proposals: Set[Proposal],
    // Decided proposals per height (at most one per height)
    decisions: Height -> Proposal,
  }

  type ChannelStateMem = {
    // Per-node: event history for the current session.
    // Cleared on fault (crash/restart). Properties fold over only
    // current-session events — no cross-session reasoning needed.
    event_history: Node -> List[ChannelEvent],
  }

  type ChannelState = {
    disk: ChannelStateDisk,
    mem: ChannelStateMem,
  }

  // ===========================================================================
  // SAFETY PROPERTIES
  // ===========================================================================

  // Agreement: For any two Decided events across all nodes, if they are for
  // the same height, the decided proposals must be identical.
  //
  // This corresponds to AgreementInv in statemachineAsync.qnt:
  //   "Any two correct processes agree in the consensus instances in which
  //    they both have already decided"
  //
  // Checked directly over event histories so it catches disagreement even
  // if a buggy generator records inconsistent decisions. After a fault,
  // histories are cleared — cross-session agreement is enforced by the
  // generator's sendDecided guard checking against the decisions map (disk).
  pure def agreement(s: ChannelState): bool = {
    // Collect all decided proposals across all nodes, grouped by height.
    // Cost: O(nodes × history_length).
    val decidedByHeight: Height -> Set[Proposal] =
      s.mem.event_history.keys().fold(Map(), (acc, node) =>
        s.mem.event_history.get(node).foldl(acc, (m, e) =>
          match e {
            | EvDecided(d) =>
                val h = d.proposal.height
                val existing = if (m.has(h)) m.get(h) else Set()
                m.put(h, existing.union(Set(d.proposal)))
            | _ => m
          }
        )
      )
    // At most one distinct proposal per height.
    decidedByHeight.keys().forall(h => decidedByHeight.get(h).size() <= 1)
  }

  // Validity: Every decided proposal was previously proposed by some node
  // via GetValue.
  //
  // This corresponds to the requirement in consensus.qnt that
  // ProposalAndCommitAndValidCInput requires a matching proposal.
  pure def validity(s: ChannelState): bool =
    s.disk.decisions.keys().forall(h =>
      s.disk.decisions.get(h).in(s.disk.all_proposals)
    )

  // Combined safety properties
  pure def safety(s: ChannelState): bool = and {
    agreement(s),
    validity(s),
  }

  // ===========================================================================
  // ORDERING PROPERTIES
  // ===========================================================================
  //
  // These properties constrain the order in which Channel API events are
  // delivered to each node. They are expressed declaratively over per-node
  // event histories, making them readable and auditable without understanding
  // any generator's internal state machine.
  //
  // Event histories contain only current-session events (cleared on fault).
  // This simplifies all fold-based properties — no session boundary logic
  // needed.
  //
  // The reference generator enforces these through its phase transitions.
  // These declarative properties let us verify that any generator (including
  // alternative or simplified ones) respects the Channel API ordering.
  //
  // Source: derived from driver.qnt, consensus.qnt, and msgs.rs documentation.

  // consensusReadyFirst: ConsensusReady is the first event in the history,
  // and it appears at most once per session.
  //
  // Before any other Channel API interaction, Malachite sends ConsensusReady
  // to initialize the node. Histories are per-session (cleared on fault),
  // so this is automatically scoped to the current session.
  //
  // Source: msgs.rs — ConsensusReady is sent when the consensus engine
  // has been initialized and is ready to start.
  pure def consensusReadyFirst(s: ChannelState): bool =
    s.mem.event_history.keys().forall(node =>
      val hist = s.mem.event_history.get(node)
      hist.foldl({ isFirst: true, ok: true }, (acc, e) =>
        if (not(acc.ok)) acc
        else match e {
          | EvConsensusReady =>
              if (not(acc.isFirst)) { isFirst: false, ok: false }
              else { isFirst: false, ok: true }
          | _ => if (acc.isFirst) { isFirst: false, ok: false } else acc
        }
      ).ok
    )

  // getValueAfterStartedRound: GetValue(h, r) is preceded by StartedRound(h, r, _)
  // for the same node.
  //
  // The proposer can only be asked to build a value after consensus has
  // entered that round. Histories are per-session (cleared on fault).
  //
  // Source: driver.qnt — GetValue is produced as part of the proposer logic
  // in applyVotekeeper, after entering a new round.
  pure def getValueAfterStartedRound(s: ChannelState): bool =
    s.mem.event_history.keys().forall(node =>
      // Scan left to right: track which (h, r) had StartedRound.
      // On GetValue(h, r), check (h, r) was already seen. Cost: O(n).
      s.mem.event_history.get(node).foldl({ seen: Set(), ok: true }, (acc, e) =>
        if (not(acc.ok)) acc
        else match e {
          | EvStartedRound(sr) =>
              { ...acc, seen: acc.seen.union(Set((sr.height, sr.round))) }
          | EvGetValue(gv) =>
              { ...acc, ok: acc.seen.contains((gv.height, gv.round)) }
          | _ => acc
        }
      ).ok
    )

  // getValueUnique: GetValue(h, r) is sent at most once globally — to at
  // most one node, at most once per node.
  //
  // Only the proposer for a given (height, round) receives GetValue, and
  // consensus requests a value exactly once per round. Combined, this means
  // each (h, r) produces at most one GetValue event across all nodes.
  //
  // Replaces the previous getValueAtMostOnce (per-node) and getValueToOneNode
  // (cross-node) with a single global uniqueness check via counting.
  //
  // Source: driver.qnt — GetValue is produced exactly when entering a round
  // where the node is the proposer, and a round is entered at most once.
  pure def getValueUnique(s: ChannelState): bool = {
    // Count GetValue events per (h, r) across all nodes.
    // Cost: O(nodes × history_length).
    val counts: (Height, Round) -> int =
      s.mem.event_history.keys().fold(Map(), (acc, node) =>
        s.mem.event_history.get(node).foldl(acc, (m, e) =>
          match e {
            | EvGetValue(gv) =>
                val key = (gv.height, gv.round)
                val cur = if (m.has(key)) m.get(key) else 0
                m.put(key, cur + 1)
            | _ => m
          }
        )
      )
    counts.keys().forall(k => counts.get(k) <= 1)
  }

  // heightMonotonic: Heights are non-decreasing in a node's consensus progress
  // events.
  //
  // Consensus processes heights sequentially. A node never receives progress
  // events for an earlier height after it has received events for a later
  // height. Query events (GetDecidedValue) are excluded — they intentionally
  // reference past heights. Histories are per-session (cleared on fault).
  //
  // Source: consensus.qnt — the state machine processes one height at a time,
  // advancing to h+1 only after deciding at h.
  pure def heightMonotonic(s: ChannelState): bool =
    s.mem.event_history.keys().forall(node =>
      // Scan left to right: track the max height seen in progress events.
      // Each new progress event must have height >= max. Cost: O(n).
      s.mem.event_history.get(node).foldl({ maxHeight: 0, ok: true }, (acc, e) =>
        if (not(acc.ok)) acc
        else if (isProgressEvent(e)) {
          match eventHeight(e) {
            | Some(h) =>
                if (h < acc.maxHeight) { ...acc, ok: false }
                else { ...acc, maxHeight: h }
            | None => acc
          }
        } else acc
      ).ok
    )

  // decidedIsFinal: After Decided(h), all subsequent consensus progress events
  // for the same node have height exactly h+1.
  //
  // Once consensus decides at height h, it moves on to height h+1. No further
  // progress events are delivered for the decided height, and no heights are
  // skipped. Query events (GetDecidedValue) are excluded — they intentionally
  // reference past heights. Histories are per-session (cleared on fault).
  //
  // Strictly stronger than just "height > maxDecided": prevents height gaps
  // (e.g., Decided(3) then StartedRound(5,0) is rejected).
  //
  // Source: driver.qnt — after producing a Decided output, the driver resets
  // to a new height. msgs.rs — Decided triggers the application to call
  // StartHeight(h+1).
  pure def decidedIsFinal(s: ChannelState): bool =
    s.mem.event_history.keys().forall(node =>
      // Scan left to right: track the max decided height.
      // Every progress event after a Decided must have height exactly
      // maxDecided + 1. Cost: O(n).
      s.mem.event_history.get(node).foldl({ maxDecided: 0, ok: true }, (acc, e) =>
        if (not(acc.ok)) acc
        else
          val check =
            if (acc.maxDecided > 0 and isProgressEvent(e)) {
              match eventHeight(e) {
                | Some(h) => h == acc.maxDecided + 1
                | None => true
              }
            } else true
          val newMax = match e {
            | EvDecided(d) => d.proposal.height
            | _ => acc.maxDecided
          }
          { maxDecided: newMax, ok: check }
      ).ok
    )

  // receivedProposalNotToProposer: A node does not receive its own proposal
  // via ReceivedProposal.
  //
  // The proposer learns its proposal through GetValue. Other nodes learn it
  // through ReceivedProposal. Malachite never sends a proposal back to the
  // node that created it.
  //
  // Source: msgs.rs — ReceivedProposalPart is delivered to non-proposer nodes.
  pure def receivedProposalNotToProposer(s: ChannelState): bool =
    s.mem.event_history.keys().forall(node =>
      val hist = s.mem.event_history.get(node)
      hist.indices().forall(i =>
        match hist[i] {
          | EvReceivedProposal(rp) => rp.proposal.proposer != node
          | _ => true
        }
      )
    )

  // syncedValueIsDecided: ProcessSyncedValue carries a proposal that was
  // decided at that height.
  //
  // The sync protocol delivers decided values. A synced proposal must match
  // the authoritative decision for its height.
  //
  // Source: msgs.rs — ProcessSyncedValue delivers a decided block for
  // catch-up. The value comes from the sync protocol which retrieves
  // decided blocks from other nodes.
  pure def syncedValueIsDecided(s: ChannelState): bool =
    s.mem.event_history.keys().forall(node =>
      val hist = s.mem.event_history.get(node)
      hist.indices().forall(i =>
        match hist[i] {
          | EvProcessSyncedValue(sv) =>
              val p = sv.proposal
              s.disk.decisions.has(p.height) and s.disk.decisions.get(p.height) == p
          | _ => true
        }
      )
    )

  // roundsIncreasingWithinHeight: Within the same height, StartedRound events
  // have strictly increasing rounds.
  //
  // Consensus processes rounds sequentially within a height. A node advances
  // to round r+1 via timeout, or skips to a higher round for a late decision.
  // It never goes back to an earlier round. Histories are per-session
  // (cleared on fault).
  //
  // Source: consensus.qnt — round transitions are forward-only (timeout
  // increments, skip-round jumps forward). driver.qnt — a round is entered
  // at most once per height.
  pure def roundsIncreasingWithinHeight(s: ChannelState): bool =
    s.mem.event_history.keys().forall(node =>
      // Scan left to right: track the last round seen per height.
      // Each new StartedRound at the same height must have a strictly
      // higher round. Cost: O(n).
      s.mem.event_history.get(node).foldl({ lastRound: Map(), ok: true }, (acc, e) =>
        if (not(acc.ok)) acc
        else match e {
          | EvStartedRound(sr) =>
              if (acc.lastRound.has(sr.height) and sr.round <= acc.lastRound.get(sr.height))
                { ...acc, ok: false }
              else
                { ...acc, lastRound: acc.lastRound.put(sr.height, sr.round) }
          | _ => acc
        }
      ).ok
    )

  // decidedRequiresDeliveredProposal: Before Decided(p), Malachite must
  // have delivered p to the node. The delivery path determines whether
  // StartedRound is also required:
  //
  //   - Sync path: ProcessSyncedValue(p) preceded Decided(p).
  //     No StartedRound required — the node is catching up.
  //
  //   - Normal path: the proposal was delivered via ReceivedProposal(p) or
  //     GetValue(p.h, p.r) as proposer, AND StartedRound(p.height, p.round)
  //     preceded Decided(p). Without round skips, consecutive round
  //     progression guarantees the node went through StartedRound(h, r)
  //     for the proposal's round.
  //
  // In the implementation, Decided contains only a certificate (2f+1 vote
  // proof), not the proposal itself. Malachite guarantees the proposal
  // has been delivered before sending the decision certificate.
  // Histories are per-session (cleared on fault).
  //
  // Source: Malachite driver — proposals are gossiped to all nodes, and the
  // proposer builds its value via GetValue.
  pure def decidedRequiresDeliveredProposal(s: ChannelState): bool =
    s.mem.event_history.keys().forall(node =>
      s.mem.event_history.get(node).foldl({
        // Proposals delivered via ProcessSyncedValue (sync path)
        syncDelivered: Set(),
        // Proposals delivered via ReceivedProposal (normal path)
        receivedProposals: Set(),
        // (h, r) where this node was asked to build a value (proposer path)
        proposedRounds: Set(),
        // (h, r) where StartedRound was seen
        startedRounds: Set(),
        ok: true,
      }, (acc, e) =>
        if (not(acc.ok)) acc
        else match e {
          | EvStartedRound(sr) =>
              { ...acc, startedRounds: acc.startedRounds.union(Set((sr.height, sr.round))) }
          | EvGetValue(gv) =>
              { ...acc, proposedRounds: acc.proposedRounds.union(Set((gv.height, gv.round))) }
          | EvReceivedProposal(rp) =>
              { ...acc, receivedProposals: acc.receivedProposals.union(Set(rp.proposal)) }
          | EvProcessSyncedValue(sv) =>
              { ...acc, syncDelivered: acc.syncDelivered.union(Set(sv.proposal)) }
          | EvDecided(d) =>
              val p = d.proposal
              // Sync path: no StartedRound required
              val viaSynced = acc.syncDelivered.contains(p)
              // Normal path: proposal delivered AND StartedRound(h, r) preceded
              val viaProposer = acc.proposedRounds.contains((p.height, p.round))
                and p.proposer == node
              val viaReceived = acc.receivedProposals.contains(p)
              val normalDelivered = viaProposer or viaReceived
              val roundStarted = acc.startedRounds.contains((p.height, p.round))
              { ...acc, ok: viaSynced or (normalDelivered and roundStarted) }
          | _ => acc
        }
      ).ok
    )

  // Combined ordering properties
  pure def ordering(s: ChannelState): bool = and {
    consensusReadyFirst(s),
    getValueAfterStartedRound(s),
    getValueUnique(s),
    heightMonotonic(s),
    decidedIsFinal(s),
    receivedProposalNotToProposer(s),
    syncedValueIsDecided(s),
    roundsIncreasingWithinHeight(s),
    decidedRequiresDeliveredProposal(s),
  }

  // ===========================================================================
  // FULL CONTRACT
  // ===========================================================================

  // All properties a conforming generator must satisfy.
  pure def contract(s: ChannelState): bool = and {
    safety(s),
    ordering(s),
  }

  // ===========================================================================
  // ASSUMPTIONS (what the application must do)
  // ===========================================================================
  //
  // Stated informally. These constrain the application's behavior.
  //
  // 1. On ConsensusReady: the application MUST reply with a starting height
  //    and validator set.
  //
  // 2. On StartedRound: the application MUST reply immediately with any
  //    known values for this round (or empty). Needed for crash recovery.
  //
  // 3. On GetValue: the application MUST reply with a proposed value within
  //    the timeout duration.
  //
  // 4. On Decided: the application MUST reply with Next (start next height
  //    or restart current height). If no reply, consensus stalls.
  //
  // 5. On GetDecidedValue: the application MUST reply with the decided
  //    value if available, or None.
  //
  // 6. On ProcessSyncedValue: the application MUST reply with the decoded
  //    value, or None.
  //
  // 7. Environment: at most f validators are faulty, where n >= 3f+1.
  //
  // 8. Environment: the network eventually delivers messages between
  //    correct validators.

  // ===========================================================================
  // LIVENESS GUARANTEES (semi-formal)
  // ===========================================================================
  //
  // Under the assumptions above:
  //
  // - For every height h, eventually Decided(h) is sent to all correct nodes.
  //
  // - The timeout mechanism (Propose -> Prevote -> Precommit -> skip round)
  //   ensures progress across rounds even when the proposer is faulty.
  //
  // - A correct proposer in a synchronous round leads to a decision.
  //
  // These are not checked as invariants (they require fairness constraints).
  // The reference generator produces finite traces that demonstrate progress
  // but does not prove liveness in general.
}
