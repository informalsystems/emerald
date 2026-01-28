//! Emerald Simplex validator node.
//!
//! Runs a simplex consensus node with EVM execution.

use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use core::num::NonZeroU32;
use core::str::FromStr;
use core::time::Duration;
use std::collections::HashMap;
use std::path::PathBuf;

use clap::{Arg, Command};
use commonware_codec::{Decode, DecodeExt};
use commonware_consensus::marshal;
use commonware_consensus::types::ViewDelta;
use commonware_cryptography::bls12381::primitives::group;
use commonware_cryptography::bls12381::primitives::sharing::Sharing;
use commonware_cryptography::bls12381::primitives::variant::MinSig;
use commonware_cryptography::ed25519::{PrivateKey, PublicKey};
use commonware_cryptography::Signer;
use commonware_p2p::authenticated::discovery as authenticated;
use commonware_p2p::{Ingress, Manager};
use commonware_runtime::{tokio, Metrics, RayonPoolSpawner, Runner};
use commonware_utils::ordered::Set;
use commonware_utils::{from_hex_formatted, union_unique, NZUsize, NZU32};
use emerald_simplex::config::SimplexConfigFile;
use emerald_simplex::consensus::NAMESPACE;
use emerald_simplex::engine::{Config as EngineConfig, Engine};
use futures::future::try_join_all;
use governor::Quota;
use malachitebft_eth_engine::engine::Engine as EmeraldEngine;
use malachitebft_eth_engine::engine_rpc::EngineRPC;
use malachitebft_eth_engine::ethereum_rpc::EthereumRPC;
use serde::{Deserialize, Serialize};
use tracing::{error, info, Level};
use url::Url;

// Channel IDs
const PENDING_CHANNEL: u64 = 0;
const RECOVERED_CHANNEL: u64 = 1;
const RESOLVER_CHANNEL: u64 = 2;
const BROADCASTER_CHANNEL: u64 = 3;
const MARSHAL_CHANNEL: u64 = 4;

// Consensus timeouts
const LEADER_TIMEOUT: Duration = Duration::from_secs(1);
const NOTARIZATION_TIMEOUT: Duration = Duration::from_secs(2);
const NULLIFY_RETRY: Duration = Duration::from_secs(10);
const FETCH_TIMEOUT: Duration = Duration::from_secs(2);
const ACTIVITY_TIMEOUT: ViewDelta = ViewDelta::new(256);
const SKIP_TIMEOUT: ViewDelta = ViewDelta::new(32);
const MAX_MESSAGE_SIZE: u32 = 1024 * 1024;
const MAX_FETCH_COUNT: usize = 16;
const MAX_FETCH_SIZE: usize = 512 * 1024;
const FETCH_CONCURRENT: usize = 4;
const BLOCKS_FREEZER_TABLE_INITIAL_SIZE: u32 = 2u32.pow(21);
const FINALIZED_FREEZER_TABLE_INITIAL_SIZE: u32 = 2u32.pow(21);

/// Epoch for validator set (fixed for now).
const EPOCH: u64 = 0;

/// Peers file format.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Peers {
    pub addresses: HashMap<String, SocketAddr>,
}

fn main() {
    // Parse arguments
    let matches = Command::new("emerald-simplex")
        .about("Emerald Simplex validator node")
        .arg(Arg::new("peers").long("peers").required(true))
        .arg(Arg::new("config").long("config").required(true))
        .get_matches();

    // Load config
    let config_file = matches.get_one::<String>("config").unwrap();
    let config_content = std::fs::read_to_string(config_file).expect("Could not read config file");
    let SimplexConfigFile {
        emerald,
        simplex: config,
    } = toml::from_str(&config_content).expect("Could not parse config file");

    // Load peers
    let peers_file = matches.get_one::<String>("peers").unwrap();
    let peers_content = std::fs::read_to_string(peers_file).expect("Could not read peers file");
    let peers: Peers = serde_yaml::from_str(&peers_content).expect("Could not parse peers file");

    // Parse private key
    let key = from_hex_formatted(&config.private_key).expect("Could not parse private key");
    let signer = PrivateKey::decode(key.as_ref()).expect("Private key is invalid");
    let public_key = signer.public_key();

    // Initialize runtime
    let cfg = tokio::Config::default()
        .with_tcp_nodelay(Some(true))
        .with_worker_threads(config.worker_threads)
        .with_storage_directory(PathBuf::from(&config.directory))
        .with_catch_panics(false);
    let executor = tokio::Runner::new(cfg);

    // Start runtime
    executor.start(|context| async move {
        // Configure telemetry
        let log_level = Level::from_str(&config.log_level).expect("Invalid log level");
        tokio::telemetry::init(
            context.with_label("telemetry"),
            tokio::telemetry::Logging {
                level: log_level,
                json: false,
            },
            Some(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                config.metrics_port,
            )),
            None,
        );

        // Build peer list
        let peers_map: HashMap<PublicKey, SocketAddr> = peers
            .addresses
            .into_iter()
            .map(|(k, v)| {
                let bytes = from_hex_formatted(&k).expect("Invalid peer key");
                let key = PublicKey::decode(bytes.as_ref()).expect("Invalid peer public key");
                (key, v)
            })
            .collect();

        let peer_keys: Vec<PublicKey> = peers_map.keys().cloned().collect();

        let ip = peers_map
            .get(&public_key)
            .expect("Could not find self in peers")
            .ip();

        // Build bootstrappers with socket addresses
        let bootstrappers: Vec<(PublicKey, Ingress)> = config
            .bootstrappers
            .iter()
            .map(|k| {
                let bytes = from_hex_formatted(k).expect("Invalid bootstrapper key");
                let key =
                    PublicKey::decode(bytes.as_ref()).expect("Invalid bootstrapper public key");
                let socket = peers_map
                    .get(&key)
                    .expect("Could not find bootstrapper in peers");
                (key, Ingress::Socket(*socket))
            })
            .collect();

        info!(peers = peer_keys.len(), "loaded peers");
        let peers_u32 = peer_keys.len() as u32;

        // Parse BLS keys
        let share = from_hex_formatted(&config.share).expect("Could not parse share");
        let share = group::Share::decode(share.as_ref()).expect("Share is invalid");
        let polynomial =
            from_hex_formatted(&config.polynomial).expect("Could not parse polynomial");
        let polynomial = Sharing::<MinSig>::decode_cfg(polynomial.as_ref(), &NZU32!(peers_u32))
            .expect("Polynomial is invalid");
        let identity = polynomial.public();

        info!(
            ?public_key,
            ?identity,
            ?ip,
            port = config.port,
            "loaded config"
        );

        // Configure network
        let p2p_namespace = union_unique(NAMESPACE, b"_P2P");
        let mut p2p_cfg = if config.local {
            authenticated::Config::local(
                signer.clone(),
                &p2p_namespace,
                SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), config.port),
                SocketAddr::new(ip, config.port),
                bootstrappers,
                MAX_MESSAGE_SIZE,
            )
        } else {
            authenticated::Config::recommended(
                signer.clone(),
                &p2p_namespace,
                SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), config.port),
                SocketAddr::new(ip, config.port),
                bootstrappers,
                MAX_MESSAGE_SIZE,
            )
        };
        p2p_cfg.mailbox_size = config.mailbox_size;

        // Start P2P
        let (mut network, mut oracle) =
            authenticated::Network::new(context.with_label("network"), p2p_cfg);

        // Provide authorized peers
        let participants: Set<PublicKey> = Set::from_iter_dedup(peer_keys.clone());
        oracle.update(EPOCH, participants.clone()).await;

        // Register channels
        let pending_limit = Quota::per_second(NonZeroU32::new(128).unwrap());
        let pending = network.register(PENDING_CHANNEL, pending_limit, config.message_backlog);

        let recovered_limit = Quota::per_second(NonZeroU32::new(128).unwrap());
        let recovered =
            network.register(RECOVERED_CHANNEL, recovered_limit, config.message_backlog);

        let resolver_limit = Quota::per_second(NonZeroU32::new(128).unwrap());
        let resolver = network.register(RESOLVER_CHANNEL, resolver_limit, config.message_backlog);

        let broadcaster_limit = Quota::per_second(NonZeroU32::new(8).unwrap());
        let broadcaster = network.register(
            BROADCASTER_CHANNEL,
            broadcaster_limit,
            config.message_backlog,
        );

        let marshal_quota = Quota::per_second(NonZeroU32::new(8).unwrap());
        let marshal = network.register(MARSHAL_CHANNEL, marshal_quota, config.message_backlog);

        // Start network
        let p2p = network.start();

        let strategy = context
            .create_strategy(NZUsize!(config.signature_threads))
            .unwrap();

        // Create marshal resolver
        let marshal_resolver_cfg = marshal::resolver::p2p::Config {
            public_key: public_key.clone(),
            manager: oracle.clone(),
            blocker: oracle.clone(),
            mailbox_size: config.mailbox_size,
            initial: Duration::from_secs(1),
            timeout: Duration::from_secs(2),
            fetch_retry_timeout: Duration::from_millis(100),
            priority_requests: false,
            priority_responses: false,
        };
        let marshal_resolver =
            marshal::resolver::p2p::init(&context, marshal_resolver_cfg, marshal);

        // Check if EVM is enabled
        if !config.evm_enabled {
            error!("EVM mode is required for emerald-simplex");
            return;
        }

        // Parse EVM config
        let engine_api_url = config
            .engine_api_url
            .clone()
            .unwrap_or_else(|| emerald.engine_authrpc_address.clone());
        let jwt_secret = config.engine_jwt_secret.clone().unwrap_or_else(|| {
            std::fs::read_to_string(&emerald.jwt_token_path)
                .expect("engine_jwt_secret or jwt_token_path required")
                .trim()
                .to_string()
        });
        let fee_recipient_hex = config
            .fee_recipient
            .clone()
            .unwrap_or_else(|| format!("0x{:x}", emerald.fee_recipient.to_alloy_address()));
        let genesis_hash_hex = config
            .genesis_execution_hash
            .clone()
            .expect("genesis_execution_hash required");

        let fee_recipient_bytes =
            from_hex_formatted(&fee_recipient_hex).expect("Invalid fee_recipient");
        let fee_recipient = alloy_primitives::Address::from_slice(&fee_recipient_bytes);

        let genesis_hash_bytes =
            from_hex_formatted(&genesis_hash_hex).expect("Invalid genesis_hash");
        let genesis_execution_hash = alloy_primitives::B256::from_slice(&genesis_hash_bytes);

        // Create Engine API client
        let engine_url = Url::parse(&engine_api_url).expect("Invalid engine_api_url");
        let eth_url_str = engine_api_url.replace("8551", "8545"); // Assume standard port offset
        let eth_url = Url::parse(&eth_url_str).expect("Invalid eth RPC URL");

        // Write JWT to temp file
        let jwt_secret = if jwt_secret.starts_with("0x") {
            jwt_secret
        } else {
            format!("0x{jwt_secret}")
        };
        let jwt_bytes = from_hex_formatted(&jwt_secret).expect("Invalid JWT secret");
        let jwt_path = PathBuf::from(&config.directory).join("jwt.hex");
        std::fs::create_dir_all(&config.directory).ok();
        std::fs::write(&jwt_path, hex::encode(&jwt_bytes)).expect("Failed to write JWT file");

        let emerald_engine = EmeraldEngine::new(
            EngineRPC::new(engine_url, &jwt_path).expect("Failed to create EngineRPC"),
            EthereumRPC::new(eth_url).expect("Failed to create EthereumRPC"),
        );

        info!("Connected to Engine API at {}", engine_api_url);

        // Create engine config
        let engine_config = EngineConfig {
            blocker: oracle.clone(),
            partition_prefix: "engine".to_string(),
            blocks_freezer_table_initial_size: BLOCKS_FREEZER_TABLE_INITIAL_SIZE,
            finalized_freezer_table_initial_size: FINALIZED_FREEZER_TABLE_INITIAL_SIZE,
            me: public_key.clone(),
            polynomial,
            share,
            participants,
            mailbox_size: config.mailbox_size,
            deque_size: config.deque_size,
            leader_timeout: LEADER_TIMEOUT,
            notarization_timeout: NOTARIZATION_TIMEOUT,
            nullify_retry: NULLIFY_RETRY,
            fetch_timeout: FETCH_TIMEOUT,
            activity_timeout: ACTIVITY_TIMEOUT,
            skip_timeout: SKIP_TIMEOUT,
            max_fetch_count: MAX_FETCH_COUNT,
            max_fetch_size: MAX_FETCH_SIZE,
            fetch_concurrent: FETCH_CONCURRENT,
            fetch_rate_per_peer: resolver_limit,
            strategy,
            engine: emerald_engine,
            fee_recipient,
            genesis_execution_hash,
            min_block_time: emerald.min_block_time,
        };

        // Create and start engine
        let engine = Engine::new(context.with_label("engine"), engine_config).await;

        info!("Starting emerald-simplex validator");

        let engine_handle =
            engine.start(pending, recovered, resolver, broadcaster, marshal_resolver);

        // Wait for completion
        let handles = vec![p2p, engine_handle];
        if let Err(e) = try_join_all(handles).await {
            error!(?e, "Validator error");
        }
    });
}
