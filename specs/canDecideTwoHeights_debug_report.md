# Witness Debug Report: `canDecideTwoHeights`

**Spec:** `emerald/specs/emerald_with_both_generators_witnesses.qnt`
**Date:** 2026-03-11
**Result:** Witness not violated by uniform random simulation; violated by biased step (scenario confirmed reachable)

---

## Goal Analysis

**Witness Definition:**
```quint
val canDecideTwoHeights: bool =
  NODES.toSet().forall(n => emeraldState.get(n).disk.last_decided_height < 2)
```

**Target Condition:** Some node with `last_decided_height >= 2`

**Required Path:**
```
init
→ height-1 full pipeline × quorum (3/4 nodes)
→ sendDecided(h=1) × quorum → FinalizeDecided → consensus_height = 2
→ StartedRound(h=2) → GetValue(node2) → newPayload × quorum
→ sendDecided(h=2) → FinalizeDecided → last_decided_height = 2
```

---

## Static Analysis

**Actions That Advance Goal:**

- **`sendDecided`** (`channel_api_generator.qnt`) — advances height after quorum
  - ✓ `phase == InRound or Syncing`
  - ✓ `proposal.height == ns.mem.height`
  - ✓ `deliveredProposals.contains(proposal)`
  - ✓ `proposalSupport.size() * 3 > NODES.length() * 2` (requires ≥3/4 nodes)

- **`sendGetValue`** (`channel_api_generator.qnt`) — creates the height-2 proposal
  - ✓ `phase == InRound at height 2`
  - ✗ `node == proposer_for(2, 0) == "node2"` ← **PROPOSER CONSTRAINT**

No static impossibility: 4 nodes, quorum = 3 — mathematically satisfiable.

---

## Dynamic Analysis

**Search Progression:**

| Pass | Samples | Steps | Result |
|------|---------|-------|--------|
| 1 | 5,000 | 500 | No violation |
| 2 | 6,000 | 600 | No violation |

**Relaxation Test:**

| | Condition |
|---|---|
| Original | `forall(n => last_decided_height < 2)` |
| Relaxed | `not(exists(n1, n2 \| n1≠n2, both last_decided_height >= 1))` |
| Result | **NOT VIOLATED** (2,000 samples × 200 steps) |

**Insight:** The blocker is upstream — two nodes never both complete height-1, so the height-2 pipeline never starts.

**Sample Traces:** One node finishes height-1 and advances, but no second node completes `FinalizeDecided(h=1)`. The Working-at-h=2 state IS reachable (GW3 violated), confirming the first node enters the height-2 round. But without the height-2 proposer (node2) completing height-1 first, no height-2 proposal is ever created.

---

## Guard Analysis

**Goal Variable:** `last_decided_height` (needs to reach >= 2)

**Guard witnesses tested:**

| Guard | Result | Meaning |
|-------|--------|---------|
| GW1: one node has `last_decided_height >= 1` and `consensus_height >= 2` | ✓ VIOLATED | One node CAN complete height-1 and advance |
| GW2: two distinct nodes both have `last_decided_height >= 1` | ✗ NOT VIOLATED | **Confirmed blocker** |
| GW3: any node in `Working` phase at `consensus_height == 2` | ✓ VIOLATED | StartedRound at height 2 is reachable |
| GW4: any node has `undecided_proposals != Set()` at `consensus_height == 2` | ✗ NOT VIOLATED | No height-2 proposal ever created |

**Blocking Guards:**

- Two distinct nodes both reaching `last_decided_height >= 1`: the height-2 quorum (3/4 nodes) requires all key nodes to complete height-1 first, but random simulation never drives enough nodes to completion.
- `undecided_proposals != Set()` at `consensus_height == 2`: no height-2 proposal is ever created because node2 (the only valid proposer at h=2, r=0) never advances past height-1 in observed traces.

**Satisfiable Guards:**

- ✓ One node reaches `last_decided_height >= 1` and `consensus_height >= 2`
- ✓ One node enters `Working` phase at `consensus_height == 2`

---

## Diagnosis

**Category:** PROBABILISTIC_TRAP

Only one node ever completes height-1 under random simulation; the remaining three are stranded at height-1, making the height-2 quorum structurally unachievable.

By the time the first node calls `sendDecided(h=1)`, the quorum guard is already satisfied (3 nodes supported the proposal), so `sendDecided(h=1)` is also enabled for the other three nodes. However, the random simulator picks uniformly across all enabled actions. Completing the remaining three `sendDecided(h=1)` calls — each followed by ~20 Engine API work steps — before making further progress on the height-2 pipeline is a specific ordering that occurs with very low probability. There is no explicit bias toward one node; the three remaining nodes are probabilistically starved in the uniform random walk.

Round timeouts do not rescue the situation. `sendStartedRoundAfterTimeout` is a per-node action: it advances only the firing node's generator from `InRound(h=2, r=0)` to `InRound(h=2, r=1)`, where `proposer_for(2,1) = "node3"`. But node3's generator is still in `InRound(h=1)` — it never completed height-1. The `sendGetValue` guard requires `phase == InRound` at the target height, so node3 cannot act as proposer at height-2 regardless of how many rounds node1 advances through. The other potential proposers at higher rounds face the same constraint.

The height-2 quorum guard (`proposalSupport.size() * 3 > NODES.length() * 2`, i.e. ≥3 nodes) is therefore unreachable: with only one node at height-2, at most one node can ever support a height-2 proposal.

**Key Evidence:** GW1 violated (one node completes h=1 and advances), GW2 NOT violated (only one node ever completes h=1 — not two), GW3 violated (Working at h=2 reachable for that one node), GW4 NOT violated (no height-2 proposal is ever created), relaxed witness also NOT violated — all consistent with probabilistic starvation of height-1 completion for nodes 2–4, not a spec bug.

---

## Biased Step

The diagnosis was confirmed by implementing a biased step action in `emerald_with_both_generators_witnesses.qnt`. The biased step corrects the scheduler distribution by restricting node selection: while any node has `last_decided_height < 1`, only those behind nodes are eligible to act. Once all four nodes complete height-1, the step reverts to uniform random selection across all nodes.

```quint
action biasedStep = {
  val behindNodes = NODES.toSet().filter(n =>
    emeraldState.get(n).disk.last_decided_height < 1
  )
  val nodePool = if (behindNodes != Set()) behindNodes else NODES.toSet()
  nondet node = nodePool.oneOf()
  any {
    stepConsensusReady(node),
    stepStartedRound(node),
    stepGetValue(node),
    nondet proposal = allKnownProposals.oneOf()
    stepReceivedProposal(node, proposal),
    nondet proposal = allKnownProposals.oneOf()
    stepDecided(node, proposal),
    stepProcessSyncedValue(node),
    stepAdvanceWork(node),
  }
}
```

**Design notes:**
- `allKnownProposals` aggregates proposals from each node's `undecided_proposals ∪ pending_proposals ∪ decided_proposals`, avoiding the need for `gen::proposals` which is not re-exported from the composition module.
- `stepGetDecidedValue` is omitted — it requires `gen::MAX_HEIGHT` (also not re-exported) and is not on the critical path to height-2.
- Fault actions (`stepNodeCrash`, `stepNodeRestart`) are omitted — a crash resets `last_decided_height` to 0, re-adding the node to `behindNodes` and trapping the simulation before the all-complete condition is ever satisfied.

**Result:** Invariant violated in 594ms (~99 traces/second, 200 samples × 500 steps).

```
quint run emerald/specs/emerald_with_both_generators_witnesses.qnt \
  --main=emerald_with_both_generators_witnesses \
  --step=biasedStep \
  --invariant=canDecideTwoHeights \
  --max-steps=500 --max-samples=200 --backend=rust
# [violation] Found an issue (594ms at 99 traces/second).
```

This confirms `canDecideTwoHeights` is a valid reachability witness. The scenario is reachable; uniform random simulation simply cannot find it because it requires all four nodes to complete the ~20-step height-1 Engine API pipeline in a coordinated order before any node advances to height-2.

---

## Where to Investigate

1. **`channel_api_generator.qnt` `sendDecided` guard** — once the quorum for height-1 is met, `sendDecided(h=1)` is enabled for all four nodes simultaneously; confirm the ~20-step Engine API work chain that must complete per node before any can advance to height-2.

2. **`channel_api_generator.qnt` `sendGetValue` / `sendStartedRoundAfterTimeout` guards** — `sendGetValue` requires `phase == InRound` at the target height; nodes still at height-1 cannot act as proposer at height-2 even if another node's timeout advances the round there. Confirm no path exists for a height-1 node to skip ahead.

3. ~~**Use a biased scheduler or manual ITF trace** to confirm reachability: force all 4 nodes to complete `sendDecided(h=1)` (and their Engine API chains) before any node's height-2 pipeline begins, then let the simulator proceed to height-2.~~ **Resolved** — see Biased Step below.
