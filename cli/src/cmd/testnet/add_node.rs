//! Add a non-validator node to an existing testnet

use core::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use core::str::FromStr;
use core::time::Duration;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use alloy_primitives::Address as AlloyAddress;
use clap::Parser;
use color_eyre::eyre::{eyre, Context as _};
use color_eyre::Result;
use malachitebft_eth_types::Address;
use tracing::info;

use super::reth::{self, RethProcess};
use super::types::RethNode;
use crate::cmd::testnet::rpc::RpcClient;
use crate::config::*;
use crate::utils::retry::retry_with_timeout;

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct TestnetAddNodeCmd {
    /// Path to the `emerald` executable. The program first checks the path provided here;
    /// if the binary is not found, it will try to resolve
    /// `emerald` from $PATH instead.
    #[clap(long, default_value = "./target/debug/emerald")]
    pub emerald_bin: String,

    /// Path to the `custom-reth` executable. The program first checks the path provided here;
    /// if the binary is not found, it will try to resolve
    /// `custom-reth` from $PATH instead.
    #[clap(long, default_value = "./custom-reth/target/debug/custom-reth")]
    pub custom_reth_bin: String,

    /// Path to reth node spawning configurations. If not specified will use default values
    #[clap(long)]
    pub reth_config_path: Option<PathBuf>,

    /// Address which will receive fees. If not specified will default to `0x4242424242424242424242424242424242424242`
    #[clap(long)]
    pub fee_receiver: Option<String>,
}

impl TestnetAddNodeCmd {
    /// Execute the add-node command
    pub fn run(&self, home_dir: &Path) -> Result<()> {
        println!("📝 Adding non-validator node to testnet...\n");

        // 1. Check if custom-reth is available
        print!("Checking custom-reth installation... ");
        match reth::check_installation(&self.custom_reth_bin) {
            Ok(version) => {
                println!("✓ {}", version.lines().next().unwrap_or(&version));
            }
            Err(e) => {
                println!("✗");
                return Err(e.wrap_err(
                    "Custom reth is not available. Make sure custom-reth/ directory exists and contains a valid reth binary."
                ));
            }
        }

        // 2. Determine the next node ID
        let node_id = self.find_next_node_id(home_dir)?;
        println!("\n📋 Next available node ID: {node_id}");

        // 3. Create node directories
        println!("\n📁 Creating node directories...");
        let node_home = home_dir.join(node_id.to_string());
        let config_dir = node_home.join("config");
        let log_dir = node_home.join("logs");
        fs::create_dir_all(&config_dir)?;
        fs::create_dir_all(&log_dir)?;
        println!("✓ Node directories created");

        // 4. Copy genesis file from existing testnet
        println!("\n📋 Copying genesis file...");
        self.copy_genesis(home_dir, node_id)?;
        println!("✓ Genesis file copied");

        // 5. Generate Malachite config
        println!("\n⚙️  Generating Malachite config...");
        self.generate_malachite_config(home_dir, node_id)?;
        println!("✓ Malachite config generated");

        let fee_receiver = if let Some(fee_receiver_str) = &self.fee_receiver {
            Address::from(AlloyAddress::from_str(fee_receiver_str)?)
        } else {
            Address::repeat_byte(42)
        };

        // 6. Generate Emerald config
        println!("\n⚙️  Generating Emerald config...");
        info!("Will use address `{fee_receiver}` as Fee Receiver address");
        self.generate_emerald_config(home_dir, node_id, fee_receiver)?;
        println!("✓ Emerald config generated");

        // 7. Generate private validator key
        println!("\n🔑 Generating private validator key...");
        self.generate_private_key(home_dir, node_id)?;
        println!("✓ Private validator key generated");

        // 8. Spawn Reth process
        println!("\n🔗 Starting Reth execution client...");
        let reth_process = self.spawn_reth_node(home_dir, node_id)?;
        println!("✓ Reth node started (PID: {})", reth_process.pid);

        // 9. Wait for Reth node to be ready
        println!("\n⏳ Waiting for Reth node to initialize...");
        let assets_dir = home_dir.join("assets");
        let reth_node = RethNode::new(
            node_id,
            home_dir.to_path_buf(),
            assets_dir,
            &self.reth_config_path,
        );
        let rpc = RpcClient::new(reth_node.ports.http);
        retry_with_timeout(
            "reth node ready",
            Duration::from_secs(30),
            Duration::from_millis(500),
            || {
                // Will succeed if the node is ready
                rpc.get_block_number()
            },
        )?;
        println!("✓ Reth node ready");

        // 10. Connect to existing peers
        println!("\n🔗 Connecting to existing peers...");
        self.connect_to_peers(home_dir, node_id)?;
        println!("✓ Connected to peers");

        // 11. Spawn Emerald process
        println!("\n💎 Starting Emerald consensus node...");
        let emerald_process = self.spawn_emerald_node(home_dir, node_id)?;
        println!("✓ Emerald node started (PID: {})", emerald_process.pid);

        println!("\n✅ Non-validator node {node_id} added successfully!");
        println!("\n📁 Logs:");
        println!("  Reth: {}/{}/logs/reth.log", home_dir.display(), node_id);
        println!(
            "  Emerald: {}/{}/logs/emerald.log",
            home_dir.display(),
            node_id
        );

        Ok(())
    }

    fn find_next_node_id(&self, home_dir: &Path) -> Result<usize> {
        if !home_dir.exists() {
            return Err(eyre!(
                "Testnet home directory does not exist: {}",
                home_dir.display()
            ));
        }

        let mut max_id = 0;
        for entry in fs::read_dir(home_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if let Ok(id) = name.parse::<usize>() {
                        max_id = max_id.max(id);
                    }
                }
            }
        }

        Ok(max_id + 1)
    }

    fn copy_genesis(&self, home_dir: &Path, node_id: usize) -> Result<()> {
        // Copy genesis.json from node 0
        let source_genesis = home_dir.join("0").join("config").join("genesis.json");
        let dest_genesis = home_dir
            .join(node_id.to_string())
            .join("config")
            .join("genesis.json");

        if !source_genesis.exists() {
            return Err(eyre!(
                "Genesis file not found at {}. Make sure the testnet is initialized.",
                source_genesis.display()
            ));
        }

        fs::copy(&source_genesis, &dest_genesis).context("Failed to copy genesis file")?;

        Ok(())
    }

    fn generate_malachite_config(&self, home_dir: &Path, node_id: usize) -> Result<()> {
        const CONSENSUS_BASE_PORT: usize = 27000;
        const MEMPOOL_BASE_PORT: usize = 28000;
        const METRICS_BASE_PORT: usize = 29000;

        // Calculate the total number of nodes (including the new one)
        let mut total_nodes = 0;
        for entry in fs::read_dir(home_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.parse::<usize>().is_ok() {
                        total_nodes += 1;
                    }
                }
            }
        }

        let consensus_port = CONSENSUS_BASE_PORT + node_id;
        let mempool_port = MEMPOOL_BASE_PORT + node_id;
        let metrics_port = METRICS_BASE_PORT + node_id;

        let transport = TransportProtocol::Tcp;

        // Generate config for non-validator node
        let config = Config {
            moniker: format!("node-{node_id}"),
            consensus: ConsensusConfig {
                timeouts: TimeoutConfig::default(),
                p2p: P2pConfig {
                    protocol: PubSubProtocol::default(),
                    listen_addr: transport.multiaddr("127.0.0.1", consensus_port),
                    persistent_peers: (0..total_nodes)
                        .filter(|j| *j != node_id)
                        .map(|j| transport.multiaddr("127.0.0.1", CONSENSUS_BASE_PORT + j))
                        .collect(),
                    discovery: DiscoveryConfig {
                        enabled: false,
                        bootstrap_protocol: BootstrapProtocol::Full,
                        selector: Selector::Random,
                        num_outbound_peers: 20,
                        num_inbound_peers: 20,
                        max_connections_per_peer: 5,
                        ephemeral_connection_timeout: Duration::from_millis(5000),
                    },
                    ..Default::default()
                },
                value_payload: ValuePayload::default(),
                queue_capacity: 0,
                ..Default::default()
            },
            mempool: MempoolConfig {
                p2p: P2pConfig {
                    protocol: PubSubProtocol::default(),
                    listen_addr: transport.multiaddr("127.0.0.1", mempool_port),
                    persistent_peers: (0..total_nodes)
                        .filter(|j| *j != node_id)
                        .map(|j| transport.multiaddr("127.0.0.1", MEMPOOL_BASE_PORT + j))
                        .collect(),
                    discovery: DiscoveryConfig {
                        enabled: false,
                        bootstrap_protocol: BootstrapProtocol::Full,
                        selector: Selector::Random,
                        num_outbound_peers: 0,
                        num_inbound_peers: 0,
                        max_connections_per_peer: 5,
                        ephemeral_connection_timeout: Duration::from_millis(5000),
                    },
                    ..Default::default()
                },
                max_tx_count: 10000,
                gossip_batch_size: 0,
                load: MempoolLoadConfig::default(),
            },
            value_sync: ValueSyncConfig {
                batch_size: 500,
                parallel_requests: 25,
                ..ValueSyncConfig::default()
            },
            metrics: MetricsConfig {
                enabled: true,
                listen_addr: SocketAddr::V4(SocketAddrV4::new(
                    Ipv4Addr::new(127, 0, 0, 1),
                    metrics_port as u16,
                )),
            },
            logging: LoggingConfig::default(),
            runtime: RuntimeConfig::SingleThreaded,
            test: TestConfig::default(),
        };

        let config_path = home_dir
            .join(node_id.to_string())
            .join("config")
            .join("config.toml");
        let config_content =
            toml::to_string_pretty(&config).context("Failed to serialize config")?;

        fs::write(&config_path, config_content).context("Failed to write config.toml")?;

        Ok(())
    }

    fn generate_emerald_config(
        &self,
        home_dir: &Path,
        node_id: usize,
        fee_receiver: Address,
    ) -> Result<()> {
        use super::types::RethPorts;

        let config_dir = home_dir.join(node_id.to_string()).join("config");
        let config_path = config_dir.join("emerald.toml");
        let ports = RethPorts::for_node(node_id);

        // JWT secret is in the assets directory
        let jwt_path = home_dir.join("assets").join("jwtsecret");

        let eth_genesis_path = home_dir.join("assets").join("genesis.json");

        // Create Emerald config for non-validator node
        let config_content = format!(
            r#"moniker = "node-{}"
el_config.execution_authrpc_address = "http://localhost:{}"
el_config.engine_authrpc_address = "http://localhost:{}"
el_config.jwt_token_path = "{}"
el_config.eth_genesis_path = "{}"
retry_config.initial_delay = "100ms"
retry_config.max_delay = "2s"
retry_config.max_elapsed_time = "20s"
el_node_type = "archive"
min_block_time = "0ms"
fee_recipient = "{}"
"#,
            node_id,
            ports.http,    // execution RPC port
            ports.authrpc, // engine auth RPC port
            jwt_path.display(),
            eth_genesis_path.display(),
            fee_receiver,
        );

        fs::write(&config_path, config_content)
            .context(format!("Failed to write Emerald config for node {node_id}"))?;

        Ok(())
    }

    fn generate_private_key(&self, home_dir: &Path, node_id: usize) -> Result<()> {
        let node_home = home_dir.join(node_id.to_string());

        // Check for built binary first, then fallback to PATH
        let emerald_bin = {
            let p = PathBuf::from(self.emerald_bin.clone());
            if p.exists() {
                p
            } else {
                PathBuf::from("emerald")
            }
        };

        info!(
            "Using `{}` for Emerald binary to generate private key",
            emerald_bin.display()
        );

        // Run emerald init to generate the private validator key
        let output = Command::new(emerald_bin)
            .args(["init", "--home"])
            .arg(&node_home)
            .output()
            .context("Failed to run emerald init")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Failed to generate private key: {}", stderr));
        }

        Ok(())
    }

    fn spawn_reth_node(&self, home_dir: &Path, node_id: usize) -> Result<RethProcess> {
        let assets_dir = home_dir.join("assets");
        let reth_node = RethNode::new(
            node_id,
            home_dir.to_path_buf(),
            assets_dir,
            &self.reth_config_path,
        );
        reth_node.spawn(&self.custom_reth_bin)
    }

    fn connect_to_peers(&self, home_dir: &Path, node_id: usize) -> Result<()> {
        let assets_dir = home_dir.join("assets");
        let new_node = RethNode::new(
            node_id,
            home_dir.to_path_buf(),
            assets_dir.clone(),
            &self.reth_config_path,
        );

        // Read and filter IDs
        let ids = fs::read_dir(home_dir)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_type()
                    .map(|file_type| file_type.is_dir())
                    .unwrap_or(false)
            })
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter_map(|name| name.parse::<usize>().ok())
            .filter(|&id| id != node_id);

        let mut connected = 0;

        // Find all existing nodes and get their enodes
        for id in ids {
            let existing_node = RethNode::new(
                id,
                home_dir.to_path_buf(),
                assets_dir.clone(),
                &self.reth_config_path,
            );
            // Try to get enode and connect
            if let Ok(enode) = existing_node.get_enode() {
                print!("  Connecting to node {id}... ");
                if new_node.add_peer(&enode).is_ok() {
                    println!("✓");
                    connected += 1;
                } else {
                    println!("✗ (skipped)");
                }
            }
        }

        if connected == 0 {
            println!("  ⚠️  No existing peers found to connect to");
        }

        Ok(())
    }

    fn spawn_emerald_node(&self, home_dir: &Path, node_id: usize) -> Result<EmeraldProcess> {
        let node_home = home_dir.join(node_id.to_string());
        let config_file = node_home.join("config").join("emerald.toml");

        // Create logs directory
        let log_dir = node_home.join("logs");
        fs::create_dir_all(&log_dir)?;

        let log_file_path = log_dir.join("emerald.log");
        let pid_file = node_home.join("emerald.pid");

        // For non-validator nodes, we don't pass a priv_validator_key.json
        // Emerald should handle this gracefully and run as a non-validator
        // Check for built binary first, then fallback to PATH
        let emerald_bin = {
            let p = PathBuf::from(self.emerald_bin.clone());
            if p.exists() {
                p
            } else {
                PathBuf::from("emerald")
            }
        };
        info!(
            "Using `{}` for Emerald binary when adding node",
            emerald_bin.display()
        );
        let cmd = format!(
            "{} start --home {} --config {} --log-level info",
            emerald_bin.display(),
            node_home.display(),
            config_file.display()
        );

        let shell_cmd = format!(
            "nohup {} > {} 2>&1 & echo $! > {}",
            cmd,
            log_file_path.display(),
            pid_file.display()
        );

        Command::new("sh")
            .arg("-c")
            .arg(&shell_cmd)
            .spawn()
            .context("Failed to spawn emerald process")?;

        // Wait a moment for PID file to be written
        std::thread::sleep(Duration::from_millis(100));

        // Read PID from file
        let pid_str = fs::read_to_string(&pid_file).context("Failed to read PID file")?;
        let pid = pid_str
            .trim()
            .parse::<u32>()
            .context("Failed to parse PID")?;

        Ok(EmeraldProcess {
            pid,
            log_file: log_file_path,
        })
    }
}

#[allow(dead_code)]
struct EmeraldProcess {
    pid: u32,
    log_file: PathBuf,
}
