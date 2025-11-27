use core::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

/// Reth node configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RethNodeConfig {
    #[serde(default = "default_log_verbosity")]
    pub log_verbosity: String,

    #[serde(default = "default_http_addr")]
    pub http_addr: String,

    #[serde(default = "default_http_corsdomain")]
    pub http_corsdomain: String,

    #[serde(default = "default_http_api")]
    pub http_api: Vec<String>,

    #[serde(default = "default_ws_addr")]
    pub ws_addr: String,

    #[serde(default = "default_authrpc_addr")]
    pub authrpc_addr: String,

    #[serde(default = "default_metrics_addr")]
    pub metrics_addr: String,

    #[serde(default = "default_nat")]
    pub nat: String,

    #[serde(default)]
    pub tx_propagation_mode: PropagationMode,

    #[serde(default = "default_txpool_pending_max_count")]
    pub txpool_pending_max_count: u32,

    #[serde(default = "default_txpool_pending_max_size")]
    pub txpool_pending_max_size: u32,

    #[serde(default = "default_txpool_queued_max_count")]
    pub txpool_queued_max_count: u32,

    #[serde(default = "default_txpool_queued_max_size")]
    pub txpool_queued_max_size: u32,

    #[serde(default = "default_txpool_max_account_slots")]
    pub txpool_max_account_slots: u32,

    #[serde(default = "default_txpool_max_batch_size")]
    pub txpool_max_batch_size: u32,

    #[serde(default = "default_txpool_gas_limit")]
    pub txpool_gas_limit: u64,

    #[serde(default = "default_max_tx_reqs")]
    pub max_tx_reqs: u32,

    #[serde(default = "default_max_tx_reqs_peer")]
    pub max_tx_reqs_peer: u32,

    #[serde(default = "default_max_pending_imports")]
    pub max_pending_imports: u32,

    #[serde(default = "default_builder_gaslimit")]
    pub builder_gaslimit: u64,

    #[serde(default = "default_builder_interval")]
    pub builder_interval: String,
}

impl Default for RethNodeConfig {
    fn default() -> Self {
        Self {
            log_verbosity: default_log_verbosity(),
            http_addr: default_http_addr(),
            http_corsdomain: default_http_corsdomain(),
            http_api: default_http_api(),
            ws_addr: default_ws_addr(),
            authrpc_addr: default_authrpc_addr(),
            metrics_addr: default_metrics_addr(),
            nat: default_nat(),
            tx_propagation_mode: PropagationMode::default(),
            txpool_pending_max_count: default_txpool_pending_max_count(),
            txpool_pending_max_size: default_txpool_pending_max_size(),
            txpool_queued_max_count: default_txpool_queued_max_count(),
            txpool_queued_max_size: default_txpool_queued_max_size(),
            txpool_max_account_slots: default_txpool_max_account_slots(),
            txpool_max_batch_size: default_txpool_max_batch_size(),
            txpool_gas_limit: default_txpool_gas_limit(),
            max_tx_reqs: default_max_tx_reqs(),
            max_tx_reqs_peer: default_max_tx_reqs_peer(),
            max_pending_imports: default_max_pending_imports(),
            builder_gaslimit: default_builder_gaslimit(),
            builder_interval: default_builder_interval(),
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PropagationMode {
    #[default]
    All,
}

impl Display for PropagationMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            Self::All => "all",
        };
        write!(f, "{s}")
    }
}

fn default_log_verbosity() -> String {
    "vvvv".into()
}
fn default_http_addr() -> String {
    "0.0.0.0".into()
}
fn default_http_corsdomain() -> String {
    "*".into()
}
fn default_http_api() -> Vec<String> {
    vec![
        "admin".into(),
        "net".into(),
        "eth".into(),
        "web3".into(),
        "debug".into(),
        "txpool".into(),
        "trace".into(),
        "ots".into(),
    ]
}

fn default_ws_addr() -> String {
    "0.0.0.0".into()
}
fn default_authrpc_addr() -> String {
    "0.0.0.0".into()
}
fn default_metrics_addr() -> String {
    "127.0.0.1".into()
}
fn default_nat() -> String {
    "extip:127.0.0.1".into()
}

fn default_txpool_pending_max_count() -> u32 {
    50_000
}
fn default_txpool_pending_max_size() -> u32 {
    500
}
fn default_txpool_queued_max_count() -> u32 {
    50_000
}
fn default_txpool_queued_max_size() -> u32 {
    500
}
fn default_txpool_max_account_slots() -> u32 {
    50_000
}
fn default_txpool_max_batch_size() -> u32 {
    10_000
}
fn default_txpool_gas_limit() -> u64 {
    3_000_000_000
}

fn default_max_tx_reqs() -> u32 {
    10_000
}
fn default_max_tx_reqs_peer() -> u32 {
    255
}
fn default_max_pending_imports() -> u32 {
    10_000
}

fn default_builder_gaslimit() -> u64 {
    66_000_000_000
}
fn default_builder_interval() -> String {
    "10ms".into()
}
