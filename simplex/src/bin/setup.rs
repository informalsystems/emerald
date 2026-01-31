//! Setup tool for emerald-simplex testnet.
//!
//! Generates configurations and keys for a local testnet.

use core::error::Error;
use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use alloy_primitives::Address as AlloyAddress;
use clap::{Parser, Subcommand};
use commonware_codec::Encode;
use commonware_cryptography::secp256r1::standard::PrivateKey;
use commonware_cryptography::Signer;
use commonware_math::algebra::Random;
use commonware_utils::{from_hex_formatted, hex};
use emerald_simplex::config::{Peers, SimplexConfig, SimplexConfigFile};
use malachitebft_eth_cli::config::{ElNodeType, EmeraldConfig};
use malachitebft_eth_types::Address;
use rand::rngs::OsRng;
use tokio::time::Duration;

/// Emerald Simplex setup tool.
#[derive(Parser)]
#[command(name = "emerald-simplex-setup")]
#[command(about = "Setup tool for emerald-simplex testnet")]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a local testnet configuration.
    Generate {
        /// Number of validators.
        #[arg(short = 'n', long, default_value = "4")]
        validators: usize,

        /// Output directory for configs.
        #[arg(short, long, default_value = "./testnet")]
        output: PathBuf,

        /// Base P2P port (validators use port, port+2, port+4, ...).
        #[arg(long, default_value = "9000")]
        base_port: u16,

        /// Engine API base port (for Reth nodes).
        #[arg(long, default_value = "8551")]
        base_engine_port: u16,

        /// Fee recipient address.
        #[arg(long, default_value = "0x4242424242424242424242424242424242424242")]
        fee_recipient: String,

        /// Path to the Ethereum genesis JSON file.
        #[arg(long, default_value = "./assets/genesis.json")]
        eth_genesis_path: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    match args.command {
        Commands::Generate {
            validators,
            output,
            base_port,
            base_engine_port,
            fee_recipient,
            eth_genesis_path,
        } => {
            generate_testnet(
                validators,
                output,
                base_port,
                base_engine_port,
                fee_recipient,
                eth_genesis_path,
            )?;
        }
    }

    Ok(())
}

fn generate_testnet(
    n: usize,
    output: PathBuf,
    base_port: u16,
    base_engine_port: u16,
    fee_recipient: String,
    eth_genesis_path: PathBuf,
) -> Result<(), Box<dyn Error>> {
    println!("Generating testnet with {n} validators...");

    // Convert to absolute path
    let output = if output.is_absolute() {
        output
    } else {
        std::env::current_dir()?.join(output)
    };

    // Check if output directory exists
    if output.exists() {
        return Err(format!("Output directory already exists: {}", output.display()).into());
    }

    // Create output directory
    fs::create_dir_all(&output)?;

    // Generate secp256r1 keys for each validator (used for both P2P and consensus)
    let mut peer_signers: Vec<PrivateKey> =
        (0..n).map(|_| PrivateKey::random(&mut OsRng)).collect();
    peer_signers.sort_by_key(|signer| signer.public_key());

    let allowed_peers: Vec<String> = peer_signers
        .iter()
        .map(|signer| hex(&signer.public_key().encode()))
        .collect();

    // Use first validator as bootstrapper
    let bootstrappers = vec![allowed_peers[0].clone()];

    println!("  Generated {n} secp256r1 key pairs");

    // Generate peers file
    let mut port = base_port;
    let mut addresses = HashMap::new();
    for signer in peer_signers.iter() {
        let name = hex(&signer.public_key().encode());
        addresses.insert(name, SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port));
        port += 2;
    }

    let peers = Peers { addresses };
    let peers_path = output.join("peers.yaml");
    fs::write(&peers_path, serde_yaml::to_string(&peers)?)?;
    println!("  Wrote peers file: {}", peers_path.display());

    let fee_recipient_bytes = from_hex_formatted(&fee_recipient).ok_or("Invalid fee recipient")?;
    let fee_recipient_addr = Address::from(AlloyAddress::from_slice(&fee_recipient_bytes));
    let base_http_port = base_engine_port.saturating_sub(6);

    // Generate config for each validator
    port = base_port;
    for (i, signer) in peer_signers.iter().enumerate() {
        let name = format!("validator-{i}");

        // Create validator directories
        let validator_dir = output.join(&name);
        let config_dir = validator_dir.join("config");
        let storage_dir = validator_dir.join("storage");
        fs::create_dir_all(&config_dir)?;
        fs::create_dir_all(&storage_dir)?;

        // Generate random JWT secret for this validator
        let jwt_secret: [u8; 32] = rand::random();
        let jwt_hex = format!("0x{}", hex::encode(jwt_secret));
        let jwt_file_path = config_dir.join("jwt.hex");
        fs::write(&jwt_file_path, &jwt_hex)?;
        println!("    Wrote JWT secret: {}", jwt_file_path.display());

        let emerald = EmeraldConfig {
            moniker: format!("simplex-{i}"),
            execution_authrpc_address: format!(
                "http://127.0.0.1:{}",
                base_http_port + (i as u16 * 100)
            ),
            engine_authrpc_address: format!(
                "http://127.0.0.1:{}",
                base_engine_port + (i as u16 * 100)
            ),
            jwt_token_path: jwt_file_path.display().to_string(),
            eth_genesis_path: eth_genesis_path.display().to_string(),
            retry_config: Default::default(),
            el_node_type: ElNodeType::Archive,
            max_retain_blocks: 0,
            prune_at_block_interval: 10,
            min_block_time: Duration::from_millis(500),
            fee_recipient: fee_recipient_addr,
        };

        let simplex = SimplexConfig {
            private_key: hex(&signer.encode()),
            port,
            metrics_port: port + 1,
            directory: storage_dir.display().to_string(),
            worker_threads: 4,
            log_level: "info".to_string(),
            local: true,
            bootstrappers: bootstrappers.clone(),
            message_backlog: 1024,
            mailbox_size: 1024,
            deque_size: 1024,
            signature_threads: 2,
            fee_recipient: None,
        };

        let config_file = SimplexConfigFile { emerald, simplex };
        let config_path = config_dir.join("validator.toml");
        fs::write(&config_path, toml::to_string_pretty(&config_file)?)?;
        println!("  Wrote config: {}", config_path.display());

        port += 2;
    }

    println!("\nTestnet configuration generated successfully!");
    println!("\nTo start the testnet:");
    println!("  1. Start {n} Reth nodes with Engine API ports:");
    for i in 0..n {
        println!(
            "     - Node {}: --authrpc.port {} --http.port {}",
            i,
            base_engine_port + (i as u16 * 100),
            8545 + (i as u16 * 100)
        );
    }
    println!("  2. Start validators:");
    for i in 0..n {
        println!(
            "     emerald-simplex --config {}/validator-{}/config/validator.toml --peers {}/peers.yaml",
            output.display(),
            i,
            output.display()
        );
    }

    Ok(())
}
