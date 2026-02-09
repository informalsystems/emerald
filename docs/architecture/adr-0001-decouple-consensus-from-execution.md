---
adr: 0001
title: decouple consensus from execution
status: Proposed
date: 2026-02-03
authors:
  - rnbguy@github
---

## Context

Emerald currently implements an EVM blockchain using Malachite consensus with
direct Engine API calls. We want to integrate Simplex consensus into the same
codebase. To avoid duplicating Engine API integration and to simplify adding new
execution layers or consensus engines, we need to decouple consensus from
execution.

## Decision

Introduce a consensus-agnostic `ExecutionLayer` trait as the single integration
boundary between consensus and execution. A single CLI binary with subcommands
selects the consensus mode.

To enable this, we restructure the codebase into separate crates for core types,
consensus implementations, and execution layer implementations. Each consensus
crate becomes a library that is generic over the `ExecutionLayer` trait, while
the CLI crate wires everything together.

### Crate Boundaries

```text
crates/
|-- core/                          # core types + ExecutionLayer trait
|   `-- src/execution_layer.rs     # trait definition
|-- consensus/
|   |-- malachite/                 # Malachite consensus (library only)
|   `-- simplex/                   # Simplex consensus (library only)
|-- execution/
|   `-- evm/                       # ExecutionLayer impl for EVM/Engine API (library only)
`-- cli/                           # single binary with subcommands
    `-- src/main.rs                # emerald malachite ... | emerald simplex ...
```

### CLI Structure

```text
emerald
|-- malachite
|   |-- init
|   |-- start
|   `-- devnet ...
`-- simplex
    |-- init
    |-- start
    `-- devnet ...
```

### Import Rules

| Crate                 | May Import         | Must Not Import           |
| --------------------- | ------------------ | ------------------------- |
| `core`                | -                  | Malachite, Simplex, Alloy |
| `consensus/malachite` | `core`, Malachite  | Simplex, Alloy            |
| `consensus/simplex`   | `core`, Commonware | Malachite, Alloy          |
| `execution/evm`       | `core`, Alloy      | Malachite, Simplex        |
| `cli`                 | all crates above   | -                         |

### Migration (from `main` branch)

| Current   | Target                | Notes                                                            |
| --------- | --------------------- | ---------------------------------------------------------------- |
| `app/`    | `consensus/malachite` | Make generic over `ExecutionLayer`, library only                 |
| `engine/` | `execution/evm`       | Implement `ExecutionLayer` trait                                 |
| `cli/`    | `cli`                 | Add simplex subcommand                                           |
| `types/`  | split                 | Shared types -> `core`, Malachite types -> `consensus/malachite` |
| (new)     | `core`                | Core trait + shared config                                       |
| (new)     | `consensus/simplex`   | Simplex consensus using `ExecutionLayer`, library only           |

> [!NOTE]
> EVM-specific directories (`utils/`, `solidity/`, `custom-reth/`) move under
> `execution/evm/`. The `malachitebft-eth-types` dependency in `utils/` should
> be replaced with types from `execution/evm` or `core`.

## Example

```rust
// core/src/execution_layer.rs
#[async_trait]
pub trait ExecutionLayer: Send + Sync {
    type Block: Send + Sync;
    type BlockId: Send + Sync;
    type ValidatorSet: Send + Sync;
    type Error: std::error::Error + Send + Sync;

    async fn genesis_block(&self) -> Result<Self::Block, Self::Error>;
    async fn build_block(&self, parent: &Self::Block, timestamp: u64) -> Result<Self::Block, Self::Error>;
    async fn validate_block(&self, block: &Self::Block) -> Result<(), Self::Error>;
    async fn finalize_block(&self, block: &Self::Block) -> Result<(), Self::Error>;
    async fn validator_set(&self, block: &Self::Block) -> Result<Self::ValidatorSet, Self::Error>;
}
```

### Before / After

**Before (app/src/app.rs):**

```rust
// Direct Engine API calls scattered throughout consensus logic
pub async fn on_get_value(/* ... */, engine: &Engine, /* ... */) -> eyre::Result<()> {
    // snip ...
    let execution_payload = engine
        .generate_block(
            &Some(latest_block),
            &emerald_config.retry_config,
            &emerald_config.fee_recipient,
            state.get_fork(latest_block.timestamp),
        )
        .await?;
    let bytes = Bytes::from(execution_payload.as_ssz_bytes());
    // snip ...
}

pub async fn on_decided(/* ... */, engine: &Engine, /* ... */) -> eyre::Result<()> {
    // snip ...
    let payload_status = engine
        .notify_new_block(execution_payload, versioned_hashes)
        .await?;
    engine
        .set_latest_forkchoice_state(block_hash, &emerald_config.retry_config)
        .await?;
    // snip ...
}
```

**After (consensus/malachite/src/app.rs):**

```rust
impl<E: ExecutionLayer> App<E> {
    pub async fn on_get_value(&mut self) -> eyre::Result<()> {
        let block = self.execution_layer.build_block(&parent, timestamp).await?;
        // snip ...
    }

    pub async fn on_decided(&mut self) -> eyre::Result<()> {
        self.execution_layer.finalize_block(&block).await?;
        // snip ...
    }
}
```

## Example Config

```toml
# emerald.toml

[global]
chain_id = 1
data_dir = "/var/lib/emerald"
log_level = "info"

# Only one execution section is allowed (enum in Rust)
[execution.evm]
engine_api_url = "http://localhost:8551"
eth_rpc_url = "http://localhost:8545"
jwt_secret_path = "/var/lib/emerald/jwt.hex"
fee_recipient = "0x0000000000000000000000000000000000000000"

# Only one consensus section is allowed (enum in Rust)
[consensus.malachite]
block_time_ms = 2000
p2p_listen = "0.0.0.0:26656"
rpc_listen = "127.0.0.1:26657"
validator_key_path = "keys/malachite.key"

# OR

[consensus.simplex]
block_time_ms = 1000
p2p_listen = "0.0.0.0:36656"
epoch_length = 100000
validator_key_path = "keys/simplex.key"
```

```rust
// core/src/config.rs
struct Config {
    global: GlobalConfig,
    execution: ExecutionConfig,
    consensus: ConsensusConfig,
}

enum ExecutionConfig {
    Evm(EvmConfig),
}

enum ConsensusConfig {
    Malachite(MalachiteConfig),
    Simplex(SimplexConfig),
}
```

## Consequences

### Pros

- New consensus engines don't need Engine API knowledge.
- Multiple EL implementations can coexist behind a single trait.
- Execution logic is testable via mock adapters.
- Clear crate boundaries reduce accidental coupling.
- Single binary simplifies deployment and distribution.
- Decoupling enables testing each component in isolation by mocking other parts.

### Cons

- Additional abstraction layer to maintain.
- Config structure changes may affect existing deployments still using `main`
  branch.

### Risks

- Malachite allows per-block validator set changes; Simplex only supports
  per-epoch changes. The `ExecutionLayer` trait must accommodate both models
  (e.g., `validator_set(block)` returning the active set, with consensus
  deciding when to query).

## Alternatives Considered

- **Keep direct Engine API in each consensus**: More duplication, harder to add
  new ELs.
- **Separate binaries per consensus**: More complex deployment, harder to share
  common CLI logic.

## References

- https://github.com/circlefin/malachite
- https://github.com/ethereum/execution-apis
- https://simplex.blog/
- https://alto.commonware.xyz/
- https://github.com/tempoxyz/tempo
