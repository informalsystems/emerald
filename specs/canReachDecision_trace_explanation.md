# Trace Explanation: `canReachDecision`

**Check Type:** witness
**Result:** VIOLATED (scenario reached ✓)
**Trace Length:** 74 steps
**Seed:** `0xecfbd9e43e8f7986`

---

## Initial State (Step 0)

All four nodes are Uninitialized (no ConsensusReady received).
The channel API and Engine API histories are empty.
Each Reth instance is Offline with only the genesis block (hash=0) in chain.

---

## Phase 1: Chaotic Initialization (Steps 1–53)

The first 53 steps are dominated by ConsensusReady, Crash, and Restart events firing in arbitrary order. No node stays stable long enough to complete a consensus round.

Key events in this phase:

| Step | Event | Effect |
|------|-------|--------|
| 1 | ConsensusReady → node4 | node4: Uninitialized → Ready, ch=1 |
| 2–3 | ConsensusReady → node3, node2 | node3, node2: Ready |
| 4 | StartedRound(h=1,r=0,proposer=node1) → node3 | node3: Working |
| 5 | Restart node3 | node3: back to Uninitialized |
| 7 | Crash node3 | wiped again |
| 8 | ConsensusReady → node1 | node1: Ready |
| 11 | Crash node1 | node1 lost |
| 12 | Restart node2 | node2 wiped |
| 17 | StartedRound(h=1,r=0,proposer=node1) → node1 | node1: Working |
| 18 | StartedRound(h=1,r=0,proposer=node1) → node2 | node2: Working |
| 19 | GetValue(h=1,r=0) → node1 | node1 is proposer, builds block payload=100 |
| 20 | Crash node1 | proposal builder crashes mid-flight |
| 21 | StartedRound(h=1,r=1,proposer=node2) → node2 | round timeout fires; node2 is proposer at r=1 |
| 40 | ReceivedProposal({h=1,payload=100,proposer=node1,r=0}) → node2 | node2 validates proposal; support count increases |
| 41 | Crash node1 | |
| 54 | Crash node1 | node1: Working → Uninitialized |
| 55 | ReceivedProposal({h=1,payload=100,proposer=node1,r=0}) → node3 | proposalSupport: {node2, node3} |
| 56–58 | ConsensusReady → node1; StartedRound(h=1,r=0) → node1, node2 | node1, node2: Working |
| 59 | ReceivedProposal({h=1,payload=100,proposer=node1,r=0}) → node4 | proposalSupport: {node2, node3, node4} = 3 nodes ✓ quorum |

Despite repeated crashes and restarts, proposal `{h=1, payload=100, proposer=node1, round=0}` (payload = height×100 + round) was built in step 19 and persists in the generator's global proposals set across all node faults. Support accumulates as nodes receive it across multiple attempts.

Quorum satisfied at step 59: `3×3 > 4×2` (9 > 8).

---

## Phase 2: Decision (Step 60)

**Step 60:** `Decided({h=1, payload=100, proposer=node1, r=0}) → node2`

| Field | Change |
|-------|--------|
| `gen::nodeStates["node2"].mem.phase` | InRound → Started |
| `gen::nodeStates["node2"].mem.height` | 1 → 2 |
| `emeraldState["node2"].mem.pendingWork` | `[]` → `[CallNewPayload, CallHeadUpdate, FinalizeDecided]` |

The generator sends `EvDecided` to node2. Emerald handles it by enqueuing three Engine API operations. The consensus decision is recorded in the generator (`gen::decisions[1] = proposal`). node2's generator phase advances to Started at height 2.

---

## Phase 3: Engine API Pipeline (Steps 61–74)

The three work items are dispatched one per `stepAdvanceWork` call. Other nodes' faults and restarts interleave but don't affect node2's queue.

**Step 61:** `stepAdvanceWork(node2)` — dispatches `CallNewPayload`

| Field | Change |
|-------|--------|
| `reth::requestHistory["node2"]` | `+= ReqNewPayload({block: {height:1, hash:1001, parentHash:0}, respStatus: Valid})` |
| `reth::rethStates["node2"].disk.chain[1]` | `{height:1, hash:1001, parentHash:0}` |
| `reth::rethStates["node2"].mem.validatedBlocks` | `{} → {1001}` |
| `emeraldState["node2"].mem.validated_cache` | `{} → {1001}` |
| `pendingWork` | `[CallHeadUpdate, FinalizeDecided]` |

Reth validates the block (hash = 1×1000+1 = 1001, parentHash=0 = genesis). Block is added to Reth's chain and marked valid. Emerald caches the validation result.

_Steps 62–70: fault/recovery events for node1, node3, node4 — node2's work queue unaffected._

**Step 71:** `stepAdvanceWork(node2)` — dispatches `CallHeadUpdate`

| Field | Change |
|-------|--------|
| `reth::requestHistory["node2"]` | `+= ReqForkchoiceUpdated({headHash: 1001, building: false, respStatus: Valid})` |
| `reth::rethStates["node2"].disk.head` | `0 → 1001` |
| `reth::rethStates["node2"].disk.headHeight` | `0 → 1` |
| `pendingWork` | `[FinalizeDecided]` |

Reth advances its canonical head to block 1001 (height 1). The execution engine now considers the decided block as the chain tip.

_Steps 72–73: ConsensusReady for node4, node1 — node2's FinalizeDecided still pending._

**Step ~74:** `stepAdvanceWork(node2)` — dispatches `FinalizeDecided`

| Field | Change |
|-------|--------|
| `emeraldState["node2"].disk.last_decided_height` | `0 → 1` |
| `emeraldState["node2"].disk.last_decided_payload` | `None → Some(100)` |
| `emeraldState["node2"].disk.latest_block` | `None → Some({hash:1001, height:1, parentHash:0})` |
| `emeraldState["node2"].disk.decided_proposals` | `Set() → Set({h:1, payload:100, proposer:node1, r:0})` |
| `emeraldState["node2"].mem.consensus_height` | `1 → 2` |
| `emeraldState["node2"].mem.phase` | `Working → Ready` |
| `emeraldState["node2"].mem.head_block_hash` | `0 → 1001` |
| `emeraldState["node2"].mem.pendingWork` | `[FinalizeDecided] → []` |

Emerald commits the decision to disk. The block built by Reth is recorded as `latest_block`. `consensus_height` advances to 2. node2 is now Ready to begin height 2.

---

## Final State (Step 74)

| Node | `last_decided_height` | `consensus_height` | `phase` |
|------|-----------------------|--------------------|---------|
| node1 | 0 | 1 | Working |
| **node2** | **1** | **2** | **Ready** |
| node3 | 0 | 0 | Uninitialized |
| node4 | 0 | 1 | Ready |

**node2 Reth:** `headHeight=1`, `chain={0→genesis, 1→{hash:1001}}`, `validatedBlocks={1001}`

✓ **SCENARIO REACHED:** `canReachDecision` violated (expected)

`node2.last_decided_height = 1 ≥ 1`, falsifying `forall(n => last_decided_height < 1)`. The three-way composition (Malachite → Emerald → Reth) successfully completed a full height-1 decision end-to-end.

---

## Summary

The trace demonstrates an end-to-end decision at height 1 via node2, despite substantial fault activity across all four nodes. Proposal `{h=1, payload=100, proposer=node1, round=0}` was built in step 19 and survived multiple node1 crashes because the generator's `proposals` set is global and persists across node faults. Quorum (3/4 nodes: node2, node3, node4) was assembled by step 59.

Once node2 received `EvDecided` (step 60), the Engine API pipeline ran across ~14 steps: `CallNewPayload` (step 61), `CallHeadUpdate` (step 71), and `FinalizeDecided` (~step 72–74). Only after all three work items completed did `last_decided_height` advance to 1, confirming the non-atomic work-queue model requires multiple transitions to finalize a single decision.

---

## Reproduction

```bash
quint run emerald/specs/emerald_with_both_generators_witnesses.qnt \
  --main=emerald_with_both_generators_witnesses \
  --invariant=canReachDecision \
  --seed=0xecfbd9e43e8f7986 \
  --max-steps=200 --max-samples=2000 \
  --verbosity=3 --backend=rust
```
