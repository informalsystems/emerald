# Emerald Specifications

Formal specifications for Emerald, a modular framework combining [Malachite](https://github.com/informalsystems/malachite) consensus with [Reth](https://github.com/paradigmxyz/reth) execution for EVM networks.

## Terminology

- **Application**: the system being modeled (Emerald). Composes with external components treated as black boxes.
- **Component**: an external black box (Malachite, Reth). Publishes a contract + generator.
- **Boundary**: the API between application and component. Has a request side and a response side.

The application can act as **server** at some boundaries (receives requests, sends responses) and **client** at others (sends requests, receives responses):

| Boundary | Who sends requests | Who sends responses |
|---|---|---|
| Channel API | Malachite (component) | Emerald (application) |
| Engine API | Emerald (application) | Reth (component) |

## Contracts

A **contract** defines properties over a shared boundary (request/response history). The contract is authoritative — it is the specification of the boundary.

Both sides make guarantees about what they produce:

- **Component guarantees**: safety + liveness on the requests it sends (if component sends requests) or on the responses it sends (if component sends responses)
- **Application guarantees**: safety + liveness on the responses it sends (if application is server) or on the requests it sends (if application is client)

Ordering properties are a subcategory of safety (constraints on valid sequences).

For example:
- **Channel API contract**: Malachite guarantees safety + ordering on the requests it sends (agreement, validity, height monotonicity). Emerald guarantees correctness on its responses (assumptions in the contract).
- **Engine API contract**: Reth guarantees safety on the responses it sends (validationStability, buildIntegrity, headProgression). Emerald guarantees ordering on the requests it sends (buildLifecycle).

### Contract Scope

Contract scope depends on the component's nature, not the boundary type:
- **Channel API**: global (cross-node) — because Malachite is a consensus protocol where safety is defined across nodes
- **Engine API**: per-node — because each Reth is an independent local process

## Generators

A **generator** is a minimal state machine that produces traces satisfying the contract. The generator is a verified reference implementation — not authoritative, but useful for composition and testing.

Generators publish:
- **Primitive actions** — one per API method (`send*` for request sources, `respond*` for request handlers)
- **Stutter action** — for steps that don't touch this boundary (`genUnchanged`, `rethUnchanged`)
- **Fault actions** — crash/restart following the disk/mem convention from `faults.qnt`
- **Pure helpers** — extracted logic reusable by the application (e.g., `computePayloadStatus`)
- **CallSpec + dispatcher** — boilerplate mapping call spec variants to generator actions (candidate for language-level automation)

Generator drive model depends on the component's role:
- **Nondeterministic** (component sends requests): the generator chooses what to send (Channel API)
- **Reactive** (component sends responses): the generator responds to application requests (Engine API)

## Composition

The application composes generators by calling their **published actions** — it never writes to generator state variables directly. Reading observable generator state (e.g., `reth::rethStates.get(node).disk.chain`) is fine.

Each composed step follows the pattern:
```
all {
  componentA::action(node),     // or componentA::unchanged
  componentB::action(node),     // or componentB::unchanged
  applicationStateUpdate(node),
}
```

Every generator's state must be accounted for in every step (either an action or the stutter/unchanged action).

### Work Queue

When a single incoming request requires multiple outgoing actions (to the same or different components) plus local state updates, they go into a sequential **work queue** (`pendingWork: List[WorkItem]`), processed one item per step:

```
Incoming request → enqueue work items → process one per step → done
```

`WorkItem` is a sum type combining:
- `EngineCall(spec)` — external call to a component (dispatched via CallSpec)
- `FinalizeX(context)` — local application state update

For example, `stepGetValue` enqueues:
```
[EngineCall(CallBuildRequest), EngineCall(CallGetPayload), FinalizeGetValue({proposal})]
```

Work items don't pass data to each other directly. They communicate through **observable component state**: the queue guarantees ordering, so each item reads the effects of prior items from the generator's state.

### Generalization

The work queue generalizes to any application that receives requests from one or more components and sends requests to one or more components:

```
                   ┌─────────────┐
   Component A ───►│             │───► Component B
   (requests in)   │ Application │     (requests out)
   Component C ───►│             │───► Component D
   (requests in)   └─────────────┘     (requests out)
```

Each external component the application sends requests to needs:
- A **CallSpec** type — one variant per action, carrying parameters (e.g., `EngineCallSpec`)
- A **dispatcher** — maps CallSpec variants to generator actions (e.g., `dispatchEngineCall`)

The application defines:
- **WorkItem** — combines CallSpec variants from all external components + local finalize handlers
- **Work queue** — `List[WorkItem]` processed one per step
- **Finalize handlers** — local state updates after external calls complete

Adding a new external component (e.g., a mempool service) means adding its `CallSpec` variants to `WorkItem` and interleaving them with existing items. No changes to the queue processing infrastructure.

Note: `CallSpec` types and dispatchers are boilerplate mechanically derivable from generator action signatures — candidates for language-level automation in Quint.

## Conventions

### Disk/Mem State Split

All state follows the convention from `faults.qnt`:
- **Disk**: survives restart, lost on crash
- **Mem**: cleared on any fault (crash or restart)

This applies uniformly across Emerald, Malachite (Channel API generator), and Reth (Engine API generator). Request/response histories are mem state.

### Naming

| Pattern | Meaning |
|---------|---------|
| `step*` | Composed transition (incoming request + enqueue work items) |
| `stepAdvanceWork` | Process next work queue item |
| `finalize*` | Local application state update (last item in a work sequence) |
| `handle*` | Direct application state update (not queue-dispatched, e.g., `handleConsensusReady`) |
| `respond*` | Engine API generator action (Reth responds to a request) |
| `send*` | Channel API generator action (Malachite sends a request) |

### Invariant Categories

Invariants are organized by which component boundaries they check:

| Category | Examples | Reads |
|----------|----------|-------|
| Emerald-only | `emerald_agreement`, `no_pending_at_current_height` | `emeraldState` only |
| Emerald ↔ Malachite | `completion` | `emeraldState` + `gen::decisions` |
| Emerald ↔ Reth | `head_tracks_consensus`, `validated_before_decided` | `emeraldState` + `reth::rethStates` |

Generator contracts (`gen::contractInv`, `reth::contractInv`) are available as commented-out invariants for full verification that the composition doesn't violate either contract.

## File Map

### Shared

| File | Purpose |
|------|---------|
| `faults.qnt` | `FaultEvent` type + disk/mem convention |

### Channel API (Malachite ↔ Emerald)

| File | Purpose |
|------|---------|
| `channel_api_contract.qnt` | Declarative properties (safety + ordering on Malachite's requests) |
| `channel_api_generator.qnt` | Reference state machine producing valid request sequences |
| `channel_api_generator_test.qnt` | Test module (3 nodes) |

### Engine API (Emerald ↔ Reth)

| File | Purpose |
|------|---------|
| `engine_api_contract.qnt` | Declarative properties (response correctness + request ordering) |
| `engine_api_generator.qnt` | Reactive state machine modeling Reth's responses |
| `engine_api_generator_test.qnt` | Test module (4 nodes, nondeterministic driver) |

### Composition

| File | Purpose |
|------|---------|
| `emerald_with_generator.qnt` | Channel API only composition (Emerald + Malachite) |
| `emerald_with_generator_test.qnt` | Test module (3 nodes) |
| `emerald_with_both_generators.qnt` | Three-way composition (Emerald + Malachite + Reth) |
| `emerald_with_both_generators_test.qnt` | Test module (4 nodes) |

### Legacy

| File | Purpose |
|------|---------|
| `emerald.qnt`, `emerald_types.qnt`, `emerald_mbt.qnt`, `emerald_tests.qnt` | Earlier MBT artifacts, not actively maintained |

## Future Work

### State Separation
Emerald currently reads generator state directly in several places (block construction, validity pre-checks, cross-component data flow, phase checks). Cleaner alternatives:
- Engine call dispatch writes responses into Emerald's own state
- Block construction becomes a work item that reads Reth state at execution time
- Phase checks use observable flags published by generators

### Independent Process Crashes
Malachite+Emerald run in one process, Reth in another. Currently crashes are coordinated. Should add independent crash/restart actions (`stepEmeraldCrash/Restart`, `stepRethCrash/Restart`). Key constraint: Reth crash must clear Emerald's work queue (deadlock otherwise) and validation cache. Invariants must gate on Reth not being offline.

### Engine API Phase 2
`exchangeCapabilities`, `getPayloadBodiesByRange/Hash`, SYNCING state.

### Language-Level Automation
`EngineCallSpec` and `dispatchEngineCall` are boilerplate that Quint could auto-generate when a spec imports a generator. Related to the `satisfies`/`assumes` annotation proposal.
