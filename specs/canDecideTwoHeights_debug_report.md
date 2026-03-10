# Witness Debug Report: `canDecideTwoHeights`

**Spec:** `emerald/specs/emerald_with_both_generators_witnesses.qnt`
**Date:** 2026-03-10
**Result:** Witness not violated (scenario not reached by random simulation)

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

Random scheduler converges on one node, starving the required proposer (node2) of height-1 completion.

Once the first node finishes the height-1 Engine API pipeline (~20+ steps), the random simulator preferentially keeps advancing that node's height-2 pipeline. "node2" — the sole valid proposer at h=2, r=0 per `proposer_for` — is perpetually stuck at height-1, so its `sendGetValue(h=2)` never fires, and no height-2 proposal ever enters any node's `undecided_proposals`.

**Key Evidence:** GW1 violated (one node completes h=1), GW2 NOT violated (two nodes never both complete h=1), GW3 violated (Working at h=2 reachable), GW4 NOT violated (no undecided proposal at h=2), relaxed witness also NOT violated — all consistent with proposer starvation, not a spec bug.

---

## Where to Investigate

1. **`channel_api_generator.qnt` `sendGetValue` guard** — `proposer_for(2,0)` is hardcoded to "node2"; confirm node2 must complete height-1 before this fires and that no other node can substitute at r=0.

2. **`emerald_with_both_generators.qnt` `stepDecided` / `finalizeDecided`** — verify the full ~20-step Engine API chain for `FinalizeDecided` and that quorum (3/4 nodes) must all complete it before any second `sendDecided` fires.

3. **Use a biased scheduler or manual ITF trace** to confirm reachability: fix scheduling so all 4 nodes complete `last_decided_height >= 1` before any node advances further, then let the simulator proceed to height-2.
