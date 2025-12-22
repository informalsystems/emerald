use std::path::Path;

use color_eyre::eyre;
use malachitebft_app::node::NodeConfig;
pub use malachitebft_config::{
    BootstrapProtocol, ConsensusConfig, DiscoveryConfig, LoggingConfig, MempoolConfig,
    MempoolLoadConfig, MetricsConfig, P2pConfig, PubSubProtocol, RuntimeConfig, ScoringStrategy,
    Selector, TestConfig, TimeoutConfig, TransportProtocol, ValuePayload, ValueSyncConfig,
};
use malachitebft_eth_types::{Address, RetryConfig};
use serde::{Deserialize, Serialize};
use tokio::time::Duration;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ElNodeType {
    /// No pruning - keeps all historical data
    #[default]
    Archive,
    /// Standard pruning - keeps recent data based on distance
    Full,
    /// Custom pruning configuration
    Custom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EmeraldConfig {
    /// A custom human-readable name for this node
    pub moniker: String,

    /// RPC endpoint of Ethereum execution client
    pub execution_authrpc_address: String,

    /// RPC endpoint of Ethereum Engine API
    pub engine_authrpc_address: String,

    /// Path of the JWT token file
    pub jwt_token_path: String,

    /// Path of the EVM genesis file
    #[serde(default = "default_eth_gensesis_path")]
    pub eth_genesis_path: String,

    /// Retry configuration for execution client sync operations
    #[serde(default)]
    pub retry_config: RetryConfig,

    /// Type of execution layer node (archive, full, or custom)
    #[serde(default)]
    pub el_node_type: ElNodeType,

    /// Number of certificates to retain.
    /// Default is retain all (u64::MAX).
    #[serde(default = "max_retain_block_default")]
    pub max_retain_blocks: u64,

    /// Number of blocks to wait before attempting pruning
    /// Note that this applies only to pruning certificates.
    /// Certificates are pruned based on max_retain_blocks.
    /// This value cannot be 0.
    /// Defatul: 10.
    #[serde(default = "prune_at_interval_default")]
    pub prune_at_block_interval: u64,
    // Application set min_block_time forcing the app to sleep
    // before moving onto the next height.
    // Malachite does not have a notion of min_block_time, thus
    // this has to be handled by the application.
    // Default: 500ms
    #[serde(with = "humantime_serde", default = "default_min_block_time")]
    pub min_block_time: Duration,

    // Address used to receive fees
    pub fee_recipient: Address,
}

fn default_min_block_time() -> Duration {
    Duration::from_millis(500)
}

fn max_retain_block_default() -> u64 {
    u64::MAX
}
fn prune_at_interval_default() -> u64 {
    10
}

fn default_eth_gensesis_path() -> String {
    "./assets/genesis.json".to_string()
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// A custom human-readable name for this node
    pub moniker: String,

    /// Consensus configuration options
    pub consensus: ConsensusConfig,

    /// Mempool configuration options
    pub mempool: MempoolConfig,

    /// ValueSync configuration options
    pub value_sync: ValueSyncConfig,

    /// Metrics configuration options
    pub metrics: MetricsConfig,

    /// Log configuration options
    pub logging: LoggingConfig,

    /// Runtime configuration options
    pub runtime: RuntimeConfig,

    /// Test configuration options
    pub test: TestConfig,
}

impl NodeConfig for Config {
    fn moniker(&self) -> &str {
        &self.moniker
    }

    fn consensus(&self) -> &ConsensusConfig {
        &self.consensus
    }

    fn consensus_mut(&mut self) -> &mut ConsensusConfig {
        &mut self.consensus
    }

    fn value_sync(&self) -> &ValueSyncConfig {
        &self.value_sync
    }

    fn value_sync_mut(&mut self) -> &mut ValueSyncConfig {
        &mut self.value_sync
    }
}

pub fn load_config(path: impl AsRef<Path>, prefix: Option<&str>) -> eyre::Result<Config> {
    ::config::Config::builder()
        .add_source(::config::File::from(path.as_ref()))
        .add_source(
            ::config::Environment::with_prefix(prefix.unwrap_or("MALACHITE")).separator("__"),
        )
        .build()?
        .try_deserialize()
        .map_err(Into::into)
}
