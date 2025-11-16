//! Testnet command

use core::str::FromStr;
use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use color_eyre::eyre::eyre;
use color_eyre::Result;
use directories::BaseDirs;
use malachitebft_app::node::{CanGeneratePrivateKey, CanMakeGenesis, CanMakePrivateKeyFile, Node};
use malachitebft_config::*;
use malachitebft_core_types::{Context, SigningScheme};
use serde::Deserialize;
use tracing::info;

use crate::args::Args;
use crate::error::Error;
use crate::file::{save_config, save_genesis, save_priv_validator_key};

type PrivateKey<C> = <<C as Context>::SigningScheme as SigningScheme>::PrivateKey;

const TESTNET_FOLDER: &str = ".malachite_testnet";

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RuntimeFlavour {
    SingleThreaded,
    MultiThreaded(usize),
}

impl FromStr for RuntimeFlavour {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains(':') {
            match s.split_once(':') {
                Some(("multi-threaded", n)) => Ok(Self::MultiThreaded(
                    n.parse()
                        .map_err(|_| "Invalid number of threads".to_string())?,
                )),
                _ => Err(format!("Invalid runtime flavour: {s}")),
            }
        } else {
            match s {
                "single-threaded" => Ok(Self::SingleThreaded),
                "multi-threaded" => Ok(Self::MultiThreaded(0)),
                _ => Err(format!("Invalid runtime flavour: {s}")),
            }
        }
    }
}

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct TestnetCmd {
    #[clap(long)]
    pub testnet_config: Option<PathBuf>,

    /// The flavor of Tokio runtime to use.
    /// Possible values:
    /// - "single-threaded": A single threaded runtime (default)
    /// - "multi-threaded:N":  A multi-threaded runtime with as N worker threads
    ///   Use a value of 0 for N to use the number of cores available on the system.
    #[clap(short, long, default_value = "single-threaded", verbatim_doc_comment)]
    pub runtime: RuntimeFlavour,

    /// Enable peer discovery.
    /// If enabled, the node will attempt to discover other nodes in the network
    #[clap(long, default_value = "false")]
    pub enable_discovery: bool,

    /// Bootstrap protocol
    /// The protocol used to bootstrap the discovery mechanism
    /// Possible values:
    /// - "kademlia": Kademlia
    /// - "full": Full mesh (default)
    #[clap(long, default_value = "full", verbatim_doc_comment)]
    pub bootstrap_protocol: BootstrapProtocol,

    /// Selector
    /// The selection strategy used to select persistent peers
    /// Possible values:
    /// - "kademlia": Kademlia-based selection, only available with the Kademlia bootstrap protocol
    /// - "random": Random selection (default)
    #[clap(long, default_value = "random", verbatim_doc_comment)]
    pub selector: Selector,

    /// Number of outbound peers
    #[clap(long, default_value = "20", verbatim_doc_comment)]
    pub num_outbound_peers: usize,

    /// Number of inbound peers
    /// Must be greater than or equal to the number of outbound peers
    #[clap(long, default_value = "20", verbatim_doc_comment)]
    pub num_inbound_peers: usize,

    /// Ephemeral connection timeout
    /// The duration in milliseconds an ephemeral connection is kept alive
    #[clap(long, default_value = "5000", verbatim_doc_comment)]
    pub ephemeral_connection_timeout_ms: u64,

    /// The transport protocol to use for P2P communication
    /// Possible values:
    /// - "tcp": TCP + Noise (default)
    /// - "quic": QUIC
    #[clap(short, long, default_value = "tcp", verbatim_doc_comment)]
    pub transport: TransportProtocol,
}

impl TestnetCmd {
    /// Execute the testnet command
    pub fn run<N>(&self, node: &N, home_dir: &Path, logging: LoggingConfig) -> Result<()>
    where
        N: Node + CanGeneratePrivateKey + CanMakeGenesis + CanMakePrivateKeyFile,
        PrivateKey<N::Context>: serde::de::DeserializeOwned,
    {
        let runtime = match self.runtime {
            RuntimeFlavour::SingleThreaded => RuntimeConfig::SingleThreaded,
            RuntimeFlavour::MultiThreaded(n) => RuntimeConfig::MultiThreaded { worker_threads: n },
        };

        let testnet_config_file = self.testnet_config.as_ref().map_or_else(
            || {
                BaseDirs::new()
                    .ok_or(eyre!("missing base directory"))
                    .map(|base_dir| base_dir.home_dir().join(TESTNET_FOLDER))
            },
            |p| Ok(p.clone()),
        )?;
        let testnet_config_content = fs::read_to_string(testnet_config_file.clone())
            .map_err(|e| Error::LoadFile(testnet_config_file.to_path_buf(), e))?;
        let testnet_config =
            toml::from_str::<TestnetConfig>(&testnet_config_content).map_err(Error::FromTOML)?;

        testnet(
            node,
            home_dir,
            &testnet_config,
            runtime,
            self.enable_discovery,
            self.bootstrap_protocol,
            self.selector,
            self.num_outbound_peers,
            self.num_inbound_peers,
            self.ephemeral_connection_timeout_ms,
            self.transport,
            logging,
        )
        .map_err(|e| eyre!("Failed to generate testnet configuration: {:?}", e))
    }
}

#[allow(clippy::too_many_arguments)]
pub fn testnet<N>(
    node: &N,
    home_dir: &Path,
    testnet_config: &TestnetConfig,
    runtime: RuntimeConfig,
    enable_discovery: bool,
    bootstrap_protocol: BootstrapProtocol,
    selector: Selector,
    num_outbound_peers: usize,
    num_inbound_peers: usize,
    ephemeral_connection_timeout_ms: u64,
    transport: TransportProtocol,
    logging: LoggingConfig,
) -> core::result::Result<(), Error>
where
    N: Node + CanGeneratePrivateKey + CanMakeGenesis + CanMakePrivateKeyFile,
    PrivateKey<N::Context>: serde::de::DeserializeOwned,
{
    let nodes = testnet_config.nodes;
    let deterministic = testnet_config.deterministic;

    // Use provided private keys if available, otherwise generate them
    let private_keys: Vec<PrivateKey<N::Context>> = if let Some(ref keys_hex) =
        testnet_config.private_keys
    {
        if keys_hex.len() != nodes {
            return Err(Error::InvalidConfig(format!(
                "Number of private keys ({}) doesn't match number of nodes ({})",
                keys_hex.len(),
                nodes
            )));
        }

        keys_hex
            .iter()
            .enumerate()
            .map(|(i, key_str)| {
                parse_private_key(node, key_str).map_err(|e| {
                    Error::InvalidConfig(format!("Failed to parse private key at index {i}: {e}"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        crate::new::generate_private_keys(node, nodes, deterministic)
    };

    let public_keys = private_keys
        .iter()
        .map(|pk| node.get_public_key(pk))
        .collect();
    let genesis = crate::new::generate_genesis(node, public_keys, deterministic);

    for (i, private_key) in private_keys.iter().enumerate().take(nodes) {
        // Use home directory `home_dir/<index>`
        let node_home_dir = home_dir.join(i.to_string());

        // Use emerald config directory `emerald_config_dir/<index>/config.toml`
        let node_emerald_config_file = testnet_config
            .configuration_paths
            .get(i)
            .ok_or(Error::MissingPath(i))?;

        let moniker = testnet_config
            .monikers
            .get(i)
            .ok_or(Error::MissingMoniker(i))?
            .clone();

        info!(
            id = %i,
            home = %node_home_dir.display(),
            emerald_config = %node_emerald_config_file.display(),
            "Generating configuration for node..."
        );

        // Set the destination folder
        let args = Args {
            home: Some(node_home_dir),
            config: Some(node_emerald_config_file.clone()),
            ..Args::default()
        };

        // Save config
        save_config(
            &args.get_config_file_path()?,
            &crate::new::generate_config(
                i,
                nodes,
                runtime,
                enable_discovery,
                bootstrap_protocol,
                selector,
                num_outbound_peers,
                num_inbound_peers,
                ephemeral_connection_timeout_ms,
                transport,
                logging,
                moniker,
            ),
        )?;

        // Save private key
        let priv_validator_key = node.make_private_key_file((*private_key).clone());
        save_priv_validator_key(
            node,
            &args.get_priv_validator_key_file_path()?,
            &priv_validator_key,
        )?;

        // Save genesis
        save_genesis(node, &args.get_genesis_file_path()?, &genesis)?;
    }

    Ok(())
}

#[derive(Deserialize)]
pub struct TestnetConfig {
    pub nodes: usize,
    pub deterministic: bool,
    pub configuration_paths: Vec<PathBuf>,
    pub monikers: Vec<String>,
    pub private_keys: Option<Vec<String>>,
}

/// Parse a private key from either:
/// 1. JSON format (as generated by `malachitebft-eth-app init`):
///    {"type": "tendermint/PrivKeySecp256k1", "value": "base64..."}
/// 2. Ethereum hex format (with or without 0x prefix): "0x1234..." or "1234..."
///
/// This function:
/// 1. Attempts to parse as JSON and extract base64 value
/// 2. Falls back to parsing as hex string
/// 3. Creates a concrete Ethereum PrivateKey from the bytes
/// 4. Converts through serde to the generic type
fn parse_private_key<N>(
    _node: &N,
    key_str: &str,
) -> core::result::Result<PrivateKey<N::Context>, String>
where
    N: Node,
    PrivateKey<N::Context>: serde::de::DeserializeOwned,
{
    use malachitebft_eth_types::secp256k1::PrivateKey as EthPrivateKey;

    // Try to parse as JSON first (init command format)
    let bytes = if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(key_str) {
        // Extract the base64 "value" field
        let base64_str = json_value
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "JSON format requires 'value' field with base64 string".to_string())?;

        // Decode base64 to bytes
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, base64_str)
            .map_err(|e| format!("Invalid base64 in 'value' field: {e}"))?
    } else {
        // Not JSON, try parsing as hex string
        let hex_str = key_str.trim().strip_prefix("0x").unwrap_or(key_str.trim());

        // Decode hex to bytes
        hex::decode(hex_str)
            .map_err(|e| format!("Invalid format: not valid JSON or hex string: {e}"))?
    };

    // Create the concrete Ethereum PrivateKey from bytes
    let eth_key =
        EthPrivateKey::from_slice(&bytes).map_err(|e| format!("Invalid private key bytes: {e}"))?;

    // Convert through serde (both types have the same serde representation)
    let json = serde_json::to_string(&eth_key)
        .map_err(|e| format!("Failed to serialize private key: {e}"))?;

    serde_json::from_str(&json).map_err(|e| format!("Failed to deserialize private key: {e}"))
}
