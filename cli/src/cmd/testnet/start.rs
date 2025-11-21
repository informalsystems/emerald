//! Testnet start command - Initialize and run a complete testnet with Reth + Emerald nodes

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use color_eyre::eyre::{eyre, Context as _};
use color_eyre::Result;
use malachitebft_app::node::{CanGeneratePrivateKey, CanMakeGenesis, CanMakePrivateKeyFile, Node};
use malachitebft_config::LoggingConfig;
use malachitebft_core_types::{Context, SigningScheme};

use super::reth::{self, RethProcess};
use super::types::RethNode;

type PrivateKey<C> = <<C as Context>::SigningScheme as SigningScheme>::PrivateKey;

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct TestnetStartCmd {
    /// Number of node pairs to create
    #[clap(short, long, default_value = "3")]
    pub nodes: usize,

    /// Private keys for validators (can be specified multiple times)
    /// Supports both hex format (0x...) and JSON format from init command
    #[clap(long = "node-keys")]
    pub node_keys: Vec<String>,

    /// Use 'cargo run --bin ...' instead of checking for built binaries
    #[clap(long)]
    pub cargo: bool,
}

impl TestnetStartCmd {
    /// Execute the testnet start command
    pub fn run<N>(&self, node: &N, home_dir: &Path, logging: LoggingConfig) -> Result<()>
    where
        N: Node + CanGeneratePrivateKey + CanMakeGenesis + CanMakePrivateKeyFile,
        PrivateKey<N::Context>: serde::de::DeserializeOwned,
    {
        println!("🚀 Initializing testnet with {} nodes...\n", self.nodes);

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

        // 2. Generate testnet configuration
        println!("\n📝 Generating testnet configuration...");
        self.generate_testnet_config(node, home_dir, logging)?;
        println!("✓ Configuration generated");

        // 2b. Generate Emerald configs
        println!("\n⚙️  Generating Emerald configs...");
        self.generate_emerald_configs(home_dir)?;
        println!("✓ Emerald configs generated");

        // 3. Extract validator public keys
        println!("\n🔑 Extracting validator public keys...");
        self.extract_public_keys(home_dir)?;
        println!("✓ Public keys extracted");

        // 4. Generate genesis file
        println!("\n⚙️  Generating genesis file...");
        self.generate_genesis(home_dir)?;
        println!("✓ Genesis file created");

        // 5. Spawn Reth processes
        println!("\n🔗 Starting Reth execution clients...");
        let reth_processes = self.spawn_reth_nodes(home_dir)?;
        println!("✓ All Reth nodes started");

        // 6. Wait for Reth nodes to be ready
        println!("\n⏳ Waiting for Reth nodes to initialize...");
        self.wait_for_reth_nodes(home_dir)?;
        println!("✓ All Reth nodes ready");

        // 7. Connect Reth peers
        println!("\n🔗 Connecting Reth peers...");
        self.connect_reth_peers(home_dir)?;
        println!("✓ Reth peers connected");

        // 8. Spawn Emerald processes
        println!("\n💎 Starting Emerald consensus nodes...");
        let emerald_processes = self.spawn_emerald_nodes(home_dir)?;
        println!("✓ All Emerald nodes started");

        println!("\n✅ Testnet started successfully!");
        println!("\n📊 Status:");
        println!("  Reth processes: {} running", reth_processes.len());
        println!("  Emerald processes: {} running", emerald_processes.len());
        println!("\n📁 Logs:");
        println!(
            "  Reth: {}/{{0..{}}}/logs/reth.log",
            home_dir.display(),
            self.nodes - 1
        );
        println!(
            "  Emerald: {}/{{0..{}}}/logs/emerald.log",
            home_dir.display(),
            self.nodes - 1
        );

        println!("\n💡 Tip: Use 'emerald testnet status' to check status");
        println!("    Use 'emerald testnet stop-node <id>' to stop a specific node");
        println!("    Use 'emerald testnet stop' to stop all nodes");

        Ok(())
    }

    fn generate_testnet_config<N>(
        &self,
        node: &N,
        home_dir: &Path,
        logging: LoggingConfig,
    ) -> Result<()>
    where
        N: Node + CanGeneratePrivateKey + CanMakeGenesis + CanMakePrivateKeyFile,
        PrivateKey<N::Context>: serde::de::DeserializeOwned,
    {
        use super::generate::{generate_testnet, TestnetConfig};
        use core::str::FromStr;
        use malachitebft_config::*;

        // Create testnet config directory
        let testnet_dir = PathBuf::from(".testnet");
        fs::create_dir_all(&testnet_dir)?;
        fs::create_dir_all(testnet_dir.join("config"))?;

        // Build configuration paths and monikers
        let mut config_paths = Vec::new();
        let mut monikers = Vec::new();
        for i in 0..self.nodes {
            // Note: emerald config is now at nodes/{N}/config/emerald.toml
            let config_path = home_dir
                .join(i.to_string())
                .join("config")
                .join("emerald.toml");
            config_paths.push(config_path);
            monikers.push(format!("node-{i}"));
        }

        let testnet_config = TestnetConfig {
            nodes: self.nodes,
            deterministic: true,
            configuration_paths: config_paths,
            monikers,
            private_keys: if self.node_keys.is_empty() {
                None
            } else {
                Some(self.node_keys.clone())
            },
        };

        // Use existing generate_testnet logic
        generate_testnet(
            node,
            home_dir,
            &testnet_config,
            RuntimeConfig::SingleThreaded,
            false, // enable_discovery
            BootstrapProtocol::from_str("full").unwrap(),
            Selector::from_str("random").unwrap(),
            20,   // num_outbound_peers
            20,   // num_inbound_peers
            5000, // ephemeral_connection_timeout_ms
            TransportProtocol::from_str("tcp").unwrap(),
            logging,
        )
        .map_err(|e| eyre!("Failed to generate testnet configuration: {:?}", e))
    }

    fn generate_emerald_configs(&self, home_dir: &Path) -> Result<()> {
        use super::types::RethPorts;

        for i in 0..self.nodes {
            let config_dir = home_dir.join(i.to_string()).join("config");
            fs::create_dir_all(&config_dir)?;

            let config_path = config_dir.join("emerald.toml");
            let ports = RethPorts::for_node(i);

            // Create Emerald config
            let config_content = format!(
                r#"moniker = "node-{}"
execution_authrpc_address = "http://localhost:{}"
engine_authrpc_address = "http://localhost:{}"
jwt_token_path = "./assets/jwtsecret"
sync_timeout_ms = 100000
sync_initial_delay_ms = 100
el_node_type = "archive"
min_block_time = "500ms"
"#,
                i,
                ports.http,    // execution RPC port
                ports.authrpc, // engine auth RPC port
            );

            fs::write(&config_path, config_content)
                .context(format!("Failed to write Emerald config for node {i}"))?;
        }

        Ok(())
    }

    fn extract_public_keys(&self, home_dir: &Path) -> Result<()> {
        let mut public_keys = Vec::new();

        for i in 0..self.nodes {
            let key_file = home_dir
                .join(i.to_string())
                .join("config")
                .join("priv_validator_key.json");

            let output = Command::new("cargo")
                .args(["run", "--bin", "emerald", "--", "show-pubkey"])
                .arg(&key_file)
                .output()
                .context("Failed to extract public key")?;

            if !output.status.success() {
                return Err(eyre!("Failed to extract public key for node {}", i));
            }

            let pubkey = String::from_utf8_lossy(&output.stdout);
            public_keys.push(pubkey.trim().to_string());
        }

        // Write to file
        let pubkeys_file = home_dir.join("validator_public_keys.txt");
        fs::write(&pubkeys_file, public_keys.join("\n"))?;

        Ok(())
    }

    fn generate_genesis(&self, home_dir: &Path) -> Result<()> {
        let pubkeys_file = home_dir.join("validator_public_keys.txt");

        let output = Command::new("cargo")
            .args([
                "run",
                "--bin",
                "emerald-utils",
                "--",
                "genesis",
                "--public-keys-file",
            ])
            .arg(&pubkeys_file)
            .args([
                "--poa-owner-address",
                "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
            ])
            .output()
            .context("Failed to generate genesis file")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Failed to generate genesis file: {}", stderr));
        }

        Ok(())
    }

    fn spawn_reth_nodes(&self, home_dir: &Path) -> Result<Vec<RethProcess>> {
        let assets_dir = PathBuf::from("./assets");
        let mut processes = Vec::new();

        for i in 0..self.nodes {
            let reth_node = RethNode::new(i, home_dir.to_path_buf(), assets_dir.clone());
            print!("  Starting Reth node {i}... ");
            let process = reth_node.spawn(self.cargo)?;
            println!("✓ (PID: {})", process.pid);
            processes.push(process);

            // Small delay between spawns
            std::thread::sleep(core::time::Duration::from_millis(500));
        }

        Ok(processes)
    }

    fn wait_for_reth_nodes(&self, home_dir: &Path) -> Result<()> {
        let assets_dir = PathBuf::from("./assets");

        for i in 0..self.nodes {
            let reth_node = RethNode::new(i, home_dir.to_path_buf(), assets_dir.clone());
            print!("  Waiting for Reth node {i} to be ready... ");
            reth_node.wait_for_ready(30)?;
            println!("✓");
        }

        Ok(())
    }

    fn connect_reth_peers(&self, home_dir: &Path) -> Result<()> {
        let assets_dir = PathBuf::from("./assets");
        let mut enodes = Vec::new();

        // Get all enodes
        for i in 0..self.nodes {
            let reth_node = RethNode::new(i, home_dir.to_path_buf(), assets_dir.clone());
            print!("  Getting enode for Reth node {i}... ");
            let enode = reth_node.get_enode()?;
            println!("✓");
            enodes.push(enode);
        }

        // Connect each node to all other nodes
        for i in 0..self.nodes {
            let reth_node = RethNode::new(i, home_dir.to_path_buf(), assets_dir.clone());
            for (j, enode) in enodes.iter().enumerate() {
                if i != j {
                    print!("  Connecting node {i} -> {j}... ");
                    reth_node.add_peer(enode)?;
                    println!("✓");
                }
            }
        }

        Ok(())
    }

    fn spawn_emerald_nodes(&self, home_dir: &Path) -> Result<Vec<EmeraldProcess>> {
        let mut processes = Vec::new();

        for i in 0..self.nodes {
            print!("  Starting Emerald node {i}... ");
            let process = self.spawn_emerald_node(i, home_dir, self.cargo)?;
            println!("✓ (PID: {})", process.pid);
            processes.push(process);

            // Small delay between spawns
            std::thread::sleep(core::time::Duration::from_millis(500));
        }

        Ok(processes)
    }

    fn spawn_emerald_node(&self, node_id: usize, home_dir: &Path, use_cargo: bool) -> Result<EmeraldProcess> {
        let node_home = home_dir.join(node_id.to_string());
        let config_file = node_home.join("config").join("emerald.toml");

        // Create logs directory
        let log_dir = node_home.join("logs");
        fs::create_dir_all(&log_dir)?;

        let log_file_path = log_dir.join("emerald.log");
        let pid_file = node_home.join("emerald.pid");

        // Create a shell command that:
        // 1. Runs in background with setsid (new session)
        // 2. Captures the actual process PID
        // 3. Writes PID to file
        let cmd = if use_cargo {
            format!(
                "cargo run --bin emerald -q -- start --home {} --config {} --log-level info",
                node_home.display(),
                config_file.display()
            )
        } else {
            // Check for built binary first, then fallback to PATH
            let debug_binary = std::path::Path::new("./target/debug/emerald");
            if debug_binary.exists() {
                format!(
                    "{} start --home {} --config {} --log-level info",
                    debug_binary.display(),
                    node_home.display(),
                    config_file.display()
                )
            } else {
                // Try PATH - will fail at spawn time if not found
                format!(
                    "emerald start --home {} --config {} --log-level info",
                    node_home.display(),
                    config_file.display()
                )
            }
        };

        let shell_cmd = format!(
            "setsid {} > {} 2>&1 & echo $! > {}",
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
        std::thread::sleep(core::time::Duration::from_millis(100));

        // Read PID from file
        let pid_str = fs::read_to_string(&pid_file)
            .context("Failed to read PID file")?;
        let pid = pid_str.trim().parse::<u32>()
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
