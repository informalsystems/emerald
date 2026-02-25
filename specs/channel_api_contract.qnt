// -*- mode: Bluespec; -*-

// =============================================================================
// Channel API Contract (DRAFT)
// =============================================================================
//
// Behavioral contract for Malachite's Channel API.
// Written by the Emerald team as a proof of concept for the contract pattern.
// Models what the Malachite team would publish for applications to consume.
//
// This contract defines:
//   1. The request types for the Channel API
//   2. Safety guarantees (agreement, validity)
//   3. Ordering guarantees (declarative properties over request histories)
//   4. Assumptions (what the application must do for guarantees to hold)
//
// Both safety and ordering properties are checked as invariants on any
// conforming generator (e.g., channel_api_generator.qnt). The generator
// enforces ordering through its state machine structure; these declarative
// properties make the ordering auditable and verifiable independently.
//
// Fault handling: Faults (crash/restart) are node-level, not API-specific.
// After a fault, the node's request history is cleared (it's ephemeral/mem
// state), while proposals and decisions survive on disk. This means contract
// properties only need to reason about the current session's requests — no
// ReqFault variant or session-reset logic needed.
//
// State follows the disk/mem convention from faults.qnt:
//   - Disk: all_proposals, decisions (survive crash/restart)
//   - Mem: request_history (cleared on fault, contains only current session)
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
  // Faults (crash/restart) are NOT part of the Channel API request type.
  // They are node-level events handled by clearing request histories.
  // See faults.qnt for the FaultEvent type.
  type ChannelRequest =
    | ReqConsensusReady
    | ReqStartedRound({ height: Height, round: Round, proposer: Node })
    | ReqGetValue({ height: Height, round: Round })
    | ReqReceivedProposal({ proposal: Proposal })
    | ReqDecided({ proposal: Proposal })
    | ReqProcessSyncedValue({ proposal: Proposal })
    | ReqGetDecidedValue({ height: Height })

  // ===========================================================================
  // REQUEST HELPERS
  // ===========================================================================

  /// Extract the height from a request, if it has one.
  pure def requestHeight(e: ChannelRequest): Option[Height] = match e {
    | ReqConsensusReady => None
    | ReqStartedRound(r) => Some(r.height)
    | ReqGetValue(r) => Some(r.height)
    | ReqReceivedProposal(r) => Some(r.proposal.height)
    | ReqDecided(r) => Some(r.proposal.height)
    | ReqProcessSyncedValue(r) => Some(r.proposal.height)
    | ReqGetDecidedValue(r) => Some(r.height)
  }

  /// Whether a request is a consensus progress request (vs. a query/delivery request).
  /// Progress requests follow monotonic height ordering and are subject to
  /// heightMonotonic and decidedIsFinal.
  /// Excluded requests:
  ///   - ReqConsensusReady: one-time initialization, not height progress
  ///   - ReqGetDecidedValue: intentionally queries past heights
  ///   - ReqReceivedProposal: can arrive for future heights (buffered as pending)
  pure def isProgressRequest(e: ChannelRequest): bool = match e {
    | ReqConsensusReady => false
    | ReqGetDecidedValue(_) => false
    | ReqReceivedProposal(_) => false
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
  //   - Mem state is cleared on fault (request histories — current session only)
  //
  // Design: request_history is the primary observable — most properties are
  // checked over it. decisions and all_proposals are kept as separate fields
  // for efficient lookup by validity and syncedValueIsDecided.

  type ChannelStateDisk = {
    // All proposals that have been made (via GetValue responses)
    all_proposals: Set[Proposal],
    // Decided proposals per height (at most one per height)
    decisions: Height -> Proposal,
    // Per-proposal: which nodes have been delivered this specific proposal
    // and are in a position to vote on it. Keyed by Proposal (not Height)
    // because different rounds at the same height can have different
    // proposals from different proposers.
    // Updated by GetValue (proposer), ReceivedProposal at current height
    // (non-proposer), and StartedRound (retroactive — if a node had the
    // proposal from a prior height as pending, counted when it reaches
    // this height). NOT updated by ProcessSyncedValue — synced values
    // are already decided, so the quorum was already met.
    // Disk state: a vote is cast when the proposal is received — this
    // doesn't un-happen on restart or crash. The network saw it.
    proposal_support: Proposal -> Set[Node],
  }

  type ChannelStateMem = {
    // Per-node: request history for the current session.
    // Cleared on fault (crash/restart). Properties fold over only
    // current-session requests — no cross-session reasoning needed.
    request_history: Node -> List[ChannelRequest],
  }

  type ChannelState = {
    disk: ChannelStateDisk,
    mem: ChannelStateMem,
  }

  // ===========================================================================
  // SAFETY PROPERTIES
  // ===========================================================================

  // Agreement: For any two Decided requests across all nodes, if they are for
  // the same height, the decided proposals must be identical.
  //
  // This corresponds to AgreementInv in statemachineAsync.qnt:
  //   "Any two correct processes agree in the consensus instances in which
  //    they both have already decided"
  //
  // Checked directly over request histories so it catches disagreement even
  // if a buggy generator records inconsistent decisions. After a fault,
  // histories are cleared — cross-session agreement is enforced by the
  // generator's sendDecided guard checking against the decisions map (disk).
  pure def agreement(s: ChannelState): bool = {
    // Collect all decided proposals across all nodes, grouped by height.
    // Cost: O(nodes × history_length).
    val decidedByHeight: Height -> Set[Proposal] =
      s.mem.request_history.keys().fold(Map(), (acc, node) =>
        s.mem.request_history.get(node).foldl(acc, (m, e) =>
          match e {
            | ReqDecided(d) =>
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

  // Decisions contiguous: Decided heights have no gaps. If height h is
  // decided, then all heights 1..h-1 are also decided.
  //
  // Consensus processes heights sequentially. A decision at height h requires
  // that height h-1 was already decided (the decided value determines the
  // app state for h). Combined with agreement, this means the global decision
  // sequence is a contiguous prefix of heights.
  //
  // Source: consensus.qnt — the state machine advances to h+1 only after
  // deciding at h. A node starts at height 1 (or its last decided + 1).
  pure def decisionsContiguous(s: ChannelState): bool =
    s.disk.decisions.keys().forall(h =>
      h == 1 or s.disk.decisions.has(h - 1)
    )

  // quorumDecision: Every decided height has proposal support from strictly
  // more than 2/3 of the network.
  //
  // In BFT consensus, a decision requires 2f+1 precommits (where n >= 3f+1),
  // which is strictly more than 2/3 of voting power. A node can only
  // precommit a proposal it has seen. At the Channel API boundary, proposal
  // delivery is observable via GetValue (proposer), ReceivedProposal at
  // current height (non-proposer), and retroactively at StartedRound
  // (for proposals received at a prior height as pending).
  // ProcessSyncedValue is NOT counted — synced proposals are already
  // decided, so the quorum was already met.
  //
  // Keyed by Proposal (not Height) because different rounds at the same
  // height can have different proposals from different proposers. The
  // quorum must hold for the specific proposal that was decided.
  //
  // This property ensures a minority partition cannot unilaterally decide.
  // The generator enforces this as a guard on sendDecided.
  //
  // Assumption: all validators have equal voting power. The quorum
  // threshold is computed as strictly more than 2/3 of node count.
  // With non-uniform voting power, this would use the validator set's
  // total voting power instead of node count.
  //
  // Source: Tendermint BFT — decision requires 2f+1 precommits where n≥3f+1.
  pure def quorumDecision(s: ChannelState, nNodes: int): bool =
    s.disk.decisions.keys().forall(h =>
      val proposal = s.disk.decisions.get(h)
      s.disk.proposal_support.has(proposal) and
      s.disk.proposal_support.get(proposal).size() * 3 > nNodes * 2
    )

  // Combined safety properties
  pure def safety(s: ChannelState, nNodes: int): bool = and {
    agreement(s),
    validity(s),
    decisionsContiguous(s),
    quorumDecision(s, nNodes),
  }

  // ===========================================================================
  // ORDERING PROPERTIES
  // ===========================================================================
  //
  // These properties constrain the order in which Channel API requests are
  // delivered to each node. They are expressed declaratively over per-node
  // request histories, making them readable and auditable without understanding
  // any generator's internal state machine.
  //
  // Request histories contain only current-session requests (cleared on fault).
  // This simplifies all fold-based properties — no session boundary logic
  // needed.
  //
  // The reference generator enforces these through its phase transitions.
  // These declarative properties let us verify that any generator (including
  // alternative or simplified ones) respects the Channel API ordering.
  //
  // Source: derived from driver.qnt, consensus.qnt, and msgs.rs documentation.

  // heightsPositive: All request heights are >= 1.
  //
  // Consensus starts at height 1 (or higher after restart). Height 0 is
  // never a valid consensus height. This is a basic structural constraint
  // that applies to all requests carrying a height.
  //
  // Source: consensus.qnt — heights start at 1. The application replies to
  // ConsensusReady with a starting height >= 1.
  pure def heightsPositive(s: ChannelState): bool =
    s.mem.request_history.keys().forall(node =>
      s.mem.request_history.get(node).foldl(true, (ok, e) =>
        ok and match requestHeight(e) {
          | Some(h) => h >= 1
          | None => true
        }
      )
    )

  // consensusReadyFirst: ConsensusReady is the first request in the history,
  // and it appears at most once per session.
  //
  // Before any other Channel API interaction, Malachite sends ConsensusReady
  // to initialize the node. Histories are per-session (cleared on fault),
  // so this is automatically scoped to the current session.
  //
  // Source: msgs.rs — ConsensusReady is sent when the consensus engine
  // has been initialized and is ready to start.
  pure def consensusReadyFirst(s: ChannelState): bool =
    s.mem.request_history.keys().forall(node =>
      val hist = s.mem.request_history.get(node)
      hist.foldl({ isFirst: true, ok: true }, (acc, e) =>
        if (not(acc.ok)) acc
        else match e {
          | ReqConsensusReady =>
              if (not(acc.isFirst)) { isFirst: false, ok: false }
              else { isFirst: false, ok: true }
          | _ => if (acc.isFirst) { isFirst: false, ok: false } else acc
        }
      ).ok
    )

  // getValueAfterStartedRound: GetValue(h, r) on node N is preceded by
  // StartedRound(h, r, proposer=N) for the same node. The node receiving
  // GetValue must have been declared as the proposer for that round.
  //
  // Strictly stronger than "preceded by StartedRound(h, r, _)": also
  // verifies that the proposer field matches the receiving node.
  // Histories are per-session (cleared on fault).
  //
  // Source: driver.qnt — GetValue is produced as part of the proposer logic
  // in applyVotekeeper, after entering a new round.
  pure def getValueAfterStartedRound(s: ChannelState): bool =
    s.mem.request_history.keys().forall(node =>
      // Scan left to right: track which (h, r) declared this node as proposer.
      // On GetValue(h, r), check (h, r) was started with proposer=self. Cost: O(n).
      s.mem.request_history.get(node).foldl({ startedAsProposer: Set(), ok: true }, (acc, e) =>
        if (not(acc.ok)) acc
        else match e {
          | ReqStartedRound(sr) =>
              if (sr.proposer == node)
                { ...acc, startedAsProposer: acc.startedAsProposer.union(Set((sr.height, sr.round))) }
              else acc
          | ReqGetValue(gv) =>
              { ...acc, ok: acc.startedAsProposer.contains((gv.height, gv.round)) }
          | _ => acc
        }
      ).ok
    )

  // getValueUnique: GetValue(h, r) is sent at most once globally — to at
  // most one node, at most once per node.
  //
  // Only the proposer for a given (height, round) receives GetValue, and
  // consensus requests a value exactly once per round. Combined, this means
  // each (h, r) produces at most one GetValue request across all nodes.
  //
  // Replaces the previous getValueAtMostOnce (per-node) and getValueToOneNode
  // (cross-node) with a single global uniqueness check via counting.
  //
  // Source: driver.qnt — GetValue is produced exactly when entering a round
  // where the node is the proposer, and a round is entered at most once.
  pure def getValueUnique(s: ChannelState): bool = {
    // Count GetValue requests per (h, r) across all nodes.
    // Cost: O(nodes × history_length).
    val counts: (Height, Round) -> int =
      s.mem.request_history.keys().fold(Map(), (acc, node) =>
        s.mem.request_history.get(node).foldl(acc, (m, e) =>
          match e {
            | ReqGetValue(gv) =>
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
  // requests.
  //
  // Consensus processes heights sequentially. A node never receives progress
  // requests for an earlier height after it has received requests for a later
  // height. Query requests (GetDecidedValue) are excluded — they intentionally
  // reference past heights. Histories are per-session (cleared on fault).
  //
  // Source: consensus.qnt — the state machine processes one height at a time,
  // advancing to h+1 only after deciding at h.
  pure def heightMonotonic(s: ChannelState): bool =
    s.mem.request_history.keys().forall(node =>
      // Scan left to right: track the max height seen in progress requests.
      // Each new progress request must have height >= max. Cost: O(n).
      s.mem.request_history.get(node).foldl({ maxHeight: 0, ok: true }, (acc, e) =>
        if (not(acc.ok)) acc
        else if (isProgressRequest(e)) {
          match requestHeight(e) {
            | Some(h) =>
                if (h < acc.maxHeight) { ...acc, ok: false }
                else { ...acc, maxHeight: h }
            | None => acc
          }
        } else acc
      ).ok
    )

  // decidedIsFinal: After Decided(h), all subsequent consensus progress requests
  // for the same node have height exactly h+1.
  //
  // Once consensus decides at height h, it moves on to height h+1. No further
  // progress requests are delivered for the decided height, and no heights are
  // skipped. Query requests (GetDecidedValue) are excluded — they intentionally
  // reference past heights. Histories are per-session (cleared on fault).
  //
  // Strictly stronger than just "height > maxDecided": prevents height gaps
  // (e.g., Decided(3) then StartedRound(5,0) is rejected).
  //
  // Source: driver.qnt — after producing a Decided output, the driver resets
  // to a new height. msgs.rs — Decided triggers the application to call
  // StartHeight(h+1).
  pure def decidedIsFinal(s: ChannelState): bool =
    s.mem.request_history.keys().forall(node =>
      // Scan left to right: track the max decided height.
      // Every progress request after a Decided must have height exactly
      // maxDecided + 1. Cost: O(n).
      s.mem.request_history.get(node).foldl({ maxDecided: 0, ok: true }, (acc, e) =>
        if (not(acc.ok)) acc
        else
          val check =
            if (acc.maxDecided > 0 and isProgressRequest(e)) {
              match requestHeight(e) {
                | Some(h) => h == acc.maxDecided + 1
                | None => true
              }
            } else true
          val newMax = match e {
            | ReqDecided(d) => d.proposal.height
            | _ => acc.maxDecided
          }
          { maxDecided: newMax, ok: check }
      ).ok
    )

  // heightAdvanceRequiresDecision: A node's consensus height only advances
  // after a decision at the previous height.
  //
  // When the height increases between progress requests, the earlier height
  // must have been decided first. This prevents height jumps without
  // decisions (e.g., StartedRound(1,0) → StartedRound(2,0) with no
  // Decided(1) in between).
  //
  // Combined with decidedIsFinal (post-decision height is exactly h+1)
  // and heightMonotonic (no regression), this fully constrains height
  // progression: heights advance one at a time, only after deciding.
  //
  // Source: consensus.qnt — the state machine advances to h+1 only after
  // deciding at h. driver.qnt — Decided triggers StartHeight(h+1).
  pure def heightAdvanceRequiresDecision(s: ChannelState): bool =
    s.mem.request_history.keys().forall(node =>
      s.mem.request_history.get(node).foldl(
        { currentHeight: 0, maxDecided: 0, ok: true }, (acc, e) =>
        if (not(acc.ok)) acc
        else
          val newMaxDecided = match e {
            | ReqDecided(d) =>
                if (d.proposal.height > acc.maxDecided) d.proposal.height
                else acc.maxDecided
            | _ => acc.maxDecided
          }
          // Track current height from progress requests.
          // After Decided(h), the node advances to h+1.
          if (isProgressRequest(e)) {
            match e {
              | ReqDecided(d) =>
                  val h = d.proposal.height + 1
                  if (acc.currentHeight > 0 and h > acc.currentHeight)
                    { currentHeight: h, maxDecided: newMaxDecided,
                      ok: acc.currentHeight <= newMaxDecided }
                  else
                    { currentHeight: h, maxDecided: newMaxDecided, ok: true }
              | _ => match requestHeight(e) {
                  | Some(h) =>
                      if (acc.currentHeight > 0 and h > acc.currentHeight)
                        { currentHeight: h, maxDecided: newMaxDecided,
                          ok: acc.currentHeight <= newMaxDecided }
                      else
                        { currentHeight: h, maxDecided: newMaxDecided, ok: true }
                  | None => { ...acc, maxDecided: newMaxDecided }
                }
            }
          } else { ...acc, maxDecided: newMaxDecided }
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
    s.mem.request_history.keys().forall(node =>
      val hist = s.mem.request_history.get(node)
      hist.indices().forall(i =>
        match hist[i] {
          | ReqReceivedProposal(rp) => rp.proposal.proposer != node
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
    s.mem.request_history.keys().forall(node =>
      val hist = s.mem.request_history.get(node)
      hist.indices().forall(i =>
        match hist[i] {
          | ReqProcessSyncedValue(sv) =>
              val p = sv.proposal
              s.disk.decisions.has(p.height) and s.disk.decisions.get(p.height) == p
          | _ => true
        }
      )
    )

  // syncRequiresStartedRound: ProcessSyncedValue(h) requires a prior
  // StartedRound(h, _) at the same height. Malachite always enters a round
  // (via start_height → StartedRound) before sync can deliver values for
  // that height. A node cannot receive a synced value for a height it hasn't
  // started consensus on.
  //
  // Source: Malachite start_height.rs — start_height always emits
  // StartedRound(h, 0) before any other requests at height h.
  pure def syncRequiresStartedRound(s: ChannelState): bool =
    s.mem.request_history.keys().forall(node =>
      // Scan left to right: track heights where StartedRound has been seen.
      // On ProcessSyncedValue(h), check StartedRound(h, _) preceded it. Cost: O(n).
      s.mem.request_history.get(node).foldl({ startedHeights: Set(), ok: true }, (acc, e) =>
        if (not(acc.ok)) acc
        else match e {
          | ReqStartedRound(sr) =>
              { ...acc, startedHeights: acc.startedHeights.union(Set(sr.height)) }
          | ReqProcessSyncedValue(sv) =>
              { ...acc, ok: acc.startedHeights.contains(sv.proposal.height) }
          | _ => acc
        }
      ).ok
    )

  // syncIsIrrevocable: After ProcessSyncedValue(h), no StartedRound or
  // GetValue requests can occur at the same height h. Once Malachite enters
  // the sync path for a height, it stays in sync until deciding — it does
  // not fall back to normal consensus rounds.
  //
  // In the implementation, once Malachite detects it is behind and receives
  // a synced value, timeouts are effectively disabled for that height.
  // The next step is always Decided, followed by height advancement (which
  // cancels all timeouts via CancelAllTimeouts + ResetTimeouts).
  //
  // Source: Malachite core-consensus sync.rs — on_value_response processes
  // the commit certificate immediately. start_height.rs — CancelAllTimeouts
  // on height advancement. timeout.rs — stale timeouts are ignored
  // (round mismatch check).
  pure def syncIsIrrevocable(s: ChannelState): bool =
    s.mem.request_history.keys().forall(node =>
      // Scan left to right: track heights where ProcessSyncedValue has been seen.
      // If StartedRound or GetValue appears at a synced height, fail. Cost: O(n).
      s.mem.request_history.get(node).foldl({ syncedHeights: Set(), ok: true }, (acc, e) =>
        if (not(acc.ok)) acc
        else match e {
          | ReqProcessSyncedValue(sv) =>
              { ...acc, syncedHeights: acc.syncedHeights.union(Set(sv.proposal.height)) }
          | ReqStartedRound(sr) =>
              { ...acc, ok: not(acc.syncedHeights.contains(sr.height)) }
          | ReqGetValue(gv) =>
              { ...acc, ok: not(acc.syncedHeights.contains(gv.height)) }
          | _ => acc
        }
      ).ok
    )

  // roundsConsecutiveWithinHeight: Within the same height, StartedRound
  // requests have consecutive rounds (exactly +1), and the first round at
  // each height is 0.
  //
  // Consensus starts each height at round 0. Rounds advance by exactly 1
  // via timeout. A node never skips rounds within normal operation.
  // Histories are per-session (cleared on fault).
  //
  // Strictly stronger than "strictly increasing": prevents round gaps
  // (e.g., round 0 → round 3). Subsumes the previous
  // roundsIncreasingWithinHeight.
  //
  // Source: consensus.qnt — timeout increments round by 1. driver.qnt —
  // each height starts at round 0.
  pure def roundsConsecutiveWithinHeight(s: ChannelState): bool =
    s.mem.request_history.keys().forall(node =>
      // Scan left to right: track the last round seen per height.
      // First StartedRound at a height must be round 0. Each subsequent
      // one must be exactly previous + 1. Cost: O(n).
      s.mem.request_history.get(node).foldl({ lastRound: Map(), ok: true }, (acc, e) =>
        if (not(acc.ok)) acc
        else match e {
          | ReqStartedRound(sr) =>
              if (acc.lastRound.has(sr.height)) {
                val prev = acc.lastRound.get(sr.height)
                { ok: sr.round == prev + 1,
                  lastRound: acc.lastRound.put(sr.height, sr.round) }
              } else {
                { ok: sr.round == 0,
                  lastRound: acc.lastRound.put(sr.height, sr.round) }
              }
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
    s.mem.request_history.keys().forall(node =>
      s.mem.request_history.get(node).foldl({
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
          | ReqStartedRound(sr) =>
              { ...acc, startedRounds: acc.startedRounds.union(Set((sr.height, sr.round))) }
          | ReqGetValue(gv) =>
              { ...acc, proposedRounds: acc.proposedRounds.union(Set((gv.height, gv.round))) }
          | ReqReceivedProposal(rp) =>
              { ...acc, receivedProposals: acc.receivedProposals.union(Set(rp.proposal)) }
          | ReqProcessSyncedValue(sv) =>
              { ...acc, syncDelivered: acc.syncDelivered.union(Set(sv.proposal)) }
          | ReqDecided(d) =>
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

  // receivedProposalExists: Every ReceivedProposal request references a
  // proposal that was previously created via GetValue.
  //
  // Malachite gossips proposals between nodes, but only proposals that
  // actually exist (were built by some proposer). A ReceivedProposal for
  // a non-existent proposal would indicate a fabrication bug.
  //
  // Source: Malachite driver — proposals are gossiped after being built
  // by the proposer via GetValue. Only existing proposals are forwarded.
  pure def receivedProposalExists(s: ChannelState): bool =
    s.mem.request_history.keys().forall(node =>
      s.mem.request_history.get(node).foldl(true, (ok, e) =>
        ok and match e {
          | ReqReceivedProposal(rp) => rp.proposal.in(s.disk.all_proposals)
          | _ => true
        }
      )
    )

  // proposerConsistency: For each (height, round), all StartedRound requests
  // across all nodes declare the same proposer.
  //
  // Proposer selection is deterministic — every node in the network is told
  // the same proposer for a given round. This catches bugs where different
  // nodes receive conflicting proposer assignments.
  //
  // Cost: O(nodes × history_length).
  pure def proposerConsistency(s: ChannelState): bool = {
    val proposersByRound: (Height, Round) -> Set[Node] =
      s.mem.request_history.keys().fold(Map(), (acc, node) =>
        s.mem.request_history.get(node).foldl(acc, (m, e) =>
          match e {
            | ReqStartedRound(sr) =>
                val key = (sr.height, sr.round)
                val existing = if (m.has(key)) m.get(key) else Set()
                m.put(key, existing.union(Set(sr.proposer)))
            | _ => m
          }
        )
      )
    proposersByRound.keys().forall(k => proposersByRound.get(k).size() <= 1)
  }

  // Combined ordering properties
  pure def ordering(s: ChannelState): bool = and {
    heightsPositive(s),
    consensusReadyFirst(s),
    getValueAfterStartedRound(s),
    getValueUnique(s),
    proposerConsistency(s),
    heightMonotonic(s),
    decidedIsFinal(s),
    heightAdvanceRequiresDecision(s),
    receivedProposalNotToProposer(s),
    receivedProposalExists(s),
    syncedValueIsDecided(s),
    syncRequiresStartedRound(s),
    syncIsIrrevocable(s),
    roundsConsecutiveWithinHeight(s),
    decidedRequiresDeliveredProposal(s),
  }

  // ===========================================================================
  // FULL CONTRACT
  // ===========================================================================

  // All properties a conforming generator must satisfy.
  pure def contract(s: ChannelState, nNodes: int): bool = and {
    safety(s, nNodes),
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
  //    the timeout duration. The proposal's height, round, and proposer
  //    fields MUST match the GetValue request (height, round, node).
  //    The payload is the application's choice.
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
  //
  // 9. Voting power: all validators have equal voting power. The quorum
  //    threshold (strictly more than 2/3 of node count) assumes this.
  //    With non-uniform voting power, quorumDecision would need the
  //    validator set's total voting power instead of nNodes.

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
