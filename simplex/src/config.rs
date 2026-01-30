use core::net::SocketAddr;
use std::collections::HashMap;

use malachitebft_eth_cli::config::EmeraldConfig;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimplexConfig {
    /// secp256r1 private key (hex-encoded) for both P2P and consensus signing.
    pub private_key: String,
    pub port: u16,
    pub metrics_port: u16,
    pub directory: String,
    pub worker_threads: usize,
    pub log_level: String,
    pub local: bool,
    pub allowed_peers: Vec<String>,
    pub bootstrappers: Vec<String>,
    pub message_backlog: usize,
    pub mailbox_size: usize,
    pub deque_size: usize,
    pub signature_threads: usize,
    pub evm_enabled: bool,
    pub engine_api_url: Option<String>,
    pub engine_jwt_secret: Option<String>,
    pub fee_recipient: Option<String>,
    pub genesis_execution_hash: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimplexConfigFile {
    #[serde(flatten)]
    pub emerald: EmeraldConfig,
    pub simplex: SimplexConfig,
}

/// Peers file format mapping public keys to socket addresses.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Peers {
    pub addresses: HashMap<String, SocketAddr>,
}
