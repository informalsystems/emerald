//! Add a non-validator node to an existing testnet

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use clap::Parser;
use color_eyre::eyre::{eyre, Context as _};
use color_eyre::Result;

use crate::config::*;
use super::reth::{self, RethProcess};
use super::types::RethNode;

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct TestnetAddNodeCmd {}

impl TestnetAddNodeCmd {
    /// Execute the add-node command
    pub fn run(&self, home_dir: &Path) -> Result<()> {
        println!("📝 Adding non-validator node to testnet...\n");

        // 1. Check if custom-reth is available
        print!("Checking custom-reth installation... ");
        match reth::check_installation() {
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
        println!("\n📋 Next available node ID: {}", node_id);

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

        // 6. Generate Emerald config
        println!("\n⚙️  Generating Emerald config...");
        self.generate_emerald_config(home_dir, node_id)?;
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
        let assets_dir = PathBuf::from("./assets");
        let reth_node = RethNode::new(node_id, home_dir.to_path_buf(), assets_dir.clone());
        reth_node.wait_for_ready(30)?;
        println!("✓ Reth node ready");

        // 10. Connect to existing peers
        println!("\n🔗 Connecting to existing peers...");
        self.connect_to_peers(home_dir, node_id)?;
        println!("✓ Connected to peers");

        // 11. Spawn Emerald process
        println!("\n💎 Starting Emerald consensus node...");
        let emerald_process = self.spawn_emerald_node(home_dir, node_id)?;
        println!("✓ Emerald node started (PID: {})", emerald_process.pid);

        println!("\n✅ Non-validator node {} added successfully!", node_id);
        println!("\n📁 Logs:");
        println!("  Reth: {}/{}/logs/reth.log", home_dir.display(), node_id);
        println!("  Emerald: {}/{}/logs/emerald.log", home_dir.display(), node_id);

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
        let dest_genesis = home_dir.join(node_id.to_string()).join("config").join("genesis.json");

        if !source_genesis.exists() {
            return Err(eyre!(
                "Genesis file not found at {}. Make sure the testnet is initialized.",
                source_genesis.display()
            ));
        }

        fs::copy(&source_genesis, &dest_genesis)
            .context("Failed to copy genesis file")?;

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
            moniker: format!("node-{}", node_id),
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
                listen_addr: format!("127.0.0.1:{}", metrics_port).parse().unwrap(),
            },
            logging: LoggingConfig::default(),
            runtime: RuntimeConfig::SingleThreaded,
            test: TestConfig::default(),
        };

        let config_path = home_dir.join(node_id.to_string()).join("config").join("config.toml");
        let config_content = toml::to_string_pretty(&config)
            .context("Failed to serialize config")?;

        fs::write(&config_path, config_content)
            .context("Failed to write config.toml")?;

        Ok(())
    }

    fn generate_emerald_config(&self, home_dir: &Path, node_id: usize) -> Result<()> {
        use super::types::RethPorts;

        let config_dir = home_dir.join(node_id.to_string()).join("config");
        let config_path = config_dir.join("emerald.toml");
        let ports = RethPorts::for_node(node_id);

        // Create Emerald config for non-validator node
        let config_content = format!(
            r#"moniker = "node-{}"
execution_authrpc_address = "http://localhost:{}"
engine_authrpc_address = "http://localhost:{}"
jwt_token_path = "./assets/jwtsecret"
sync_timeout_ms = 10000
sync_initial_delay_ms = 100
el_node_type = "archive"
"#,
            node_id,
            ports.http,    // execution RPC port
            ports.authrpc, // engine auth RPC port
        );

        fs::write(&config_path, config_content)
            .context(format!("Failed to write Emerald config for node {}", node_id))?;

        Ok(())
    }

    fn generate_private_key(&self, home_dir: &Path, node_id: usize) -> Result<()> {
        let node_home = home_dir.join(node_id.to_string());

        // Run emerald init to generate the private validator key
        let output = Command::new("cargo")
            .args([
                "run",
                "--bin",
                "emerald",
                "-q",
                "--",
                "init",
                "--home",
            ])
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
        let assets_dir = PathBuf::from("./assets");
        let reth_node = RethNode::new(node_id, home_dir.to_path_buf(), assets_dir);
        reth_node.spawn()
    }

    fn connect_to_peers(&self, home_dir: &Path, node_id: usize) -> Result<()> {
        let assets_dir = PathBuf::from("./assets");
        let new_node = RethNode::new(node_id, home_dir.to_path_buf(), assets_dir.clone());

        // Find all existing nodes and get their enodes
        let mut connected = 0;
        for entry in fs::read_dir(home_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if let Ok(id) = name.parse::<usize>() {
                        if id != node_id {
                            let existing_node = RethNode::new(id, home_dir.to_path_buf(), assets_dir.clone());
                            // Try to get enode and connect
                            if let Ok(enode) = existing_node.get_enode() {
                                print!("  Connecting to node {}... ", id);
                                if new_node.add_peer(&enode).is_ok() {
                                    println!("✓");
                                    connected += 1;
                                } else {
                                    println!("✗ (skipped)");
                                }
                            }
                        }
                    }
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
        let log_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file_path)?;

        // For non-validator nodes, we don't pass a priv_validator_key.json
        // Emerald should handle this gracefully and run as a non-validator
        let child = Command::new("cargo")
            .args([
                "run",
                "--bin",
                "emerald",
                "-q",
                "--",
                "start",
                "--home",
            ])
            .arg(&node_home)
            .arg("--config")
            .arg(&config_file)
            .arg("--log-level")
            .arg("info")
            .stdout(Stdio::from(log_file.try_clone()?))
            .stderr(Stdio::from(log_file))
            .spawn()
            .context("Failed to spawn emerald process")?;

        let pid = child.id();

        // Write PID to file
        let pid_file = node_home.join("emerald.pid");
        fs::write(&pid_file, pid.to_string())?;

        Ok(EmeraldProcess {
            child,
            pid,
            log_file: log_file_path,
        })
    }
}

#[allow(dead_code)]
struct EmeraldProcess {
    child: std::process::Child,
    pid: u32,
    log_file: PathBuf,
}
