//! Testnet start command - Initialize and run a complete testnet with Reth + Emerald nodes

use core::time::Duration;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use color_eyre::eyre::{eyre, Context as _};
use color_eyre::Result;
use malachitebft_app::node::{CanGeneratePrivateKey, CanMakeGenesis, CanMakePrivateKeyFile, Node};
use malachitebft_config::LoggingConfig;
use malachitebft_core_types::{Context, SigningScheme};
use tracing::info;

use super::reth::{self, RethProcess};
use super::types::RethNode;
use crate::cmd::testnet::rpc::RpcClient;
use crate::utils::retry::retry_with_timeout;

type PrivateKey<C> = <<C as Context>::SigningScheme as SigningScheme>::PrivateKey;

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct TestnetStartCmd {
    /// Number of node pairs to create (max 20)
    #[clap(short, long, default_value = "3")]
    pub nodes: usize,

    /// Private keys for validators (can be specified multiple times)
    /// Supports both hex format (0x...) and JSON format from init command
    #[clap(long = "node-keys")]
    pub node_keys: Option<Vec<String>>,

    /// Path to `emerald` binary. If not specified will default to `./target/debug/emerald`
    #[clap(long, default_value = "./target/debug/emerald")]
    pub emerald_bin: String,

    /// Path to `emerald-utils` binary. If not specified will default to `./target/debug/emerald-utils`
    #[clap(long, default_value = "./target/debug/emerald-utils")]
    pub emerald_utils_bin: String,

    /// Path to `custom-reth` binary. If not specified will default to `./custom-reth/target/debug/custom-reth`
    #[clap(long, default_value = "./custom-reth/target/debug/custom-reth")]
    pub custom_reth_bin: String,

    /// Path to reth node spawning configurations. If not specified will use default values
    #[clap(long)]
    pub reth_config_path: Option<PathBuf>,
}

impl TestnetStartCmd {
    /// Execute the testnet start command
    pub fn run<N>(&self, node: &N, home_dir: &Path, logging: LoggingConfig) -> Result<()>
    where
        N: Node + CanGeneratePrivateKey + CanMakeGenesis + CanMakePrivateKeyFile,
        PrivateKey<N::Context>: serde::de::DeserializeOwned,
    {
        // Validate node count
        if self.nodes == 0 || self.nodes > 20 {
            return Err(eyre!(
                "Number of nodes must be between 1 and 20 (got {})",
                self.nodes
            ));
        }

        println!("🚀 Initializing testnet with {} nodes...\n", self.nodes);

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

        // 2. Generate testnet configuration
        println!("\n📝 Generating testnet configuration...");
        self.generate_testnet_config(node, home_dir, logging)?;
        println!("✓ Configuration generated");

        // 2b. Set up assets directory
        println!("\n📦 Setting up assets directory...");
        self.setup_assets_directory(home_dir)?;
        println!("✓ Assets directory set up");

        // 2c. Generate Emerald configs
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

        println!("\n💡 Commands:");
        println!("    emerald testnet status           - Check status of all nodes");
        println!("    emerald testnet stop-node <id>   - Stop a specific node");
        println!("    emerald testnet stop             - Stop all nodes");
        println!("    emerald testnet rm               - Remove all testnet data");

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
        use malachitebft_config::*;

        use super::generate::{generate_testnet, TestnetConfig};

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
            private_keys: self.node_keys.clone(),
        };

        // Use existing generate_testnet logic
        generate_testnet(
            node,
            home_dir,
            &testnet_config,
            RuntimeConfig::SingleThreaded,
            false, // enable_discovery
            BootstrapProtocol::Full,
            Selector::Random,
            20,   // num_outbound_peers
            20,   // num_inbound_peers
            5000, // ephemeral_connection_timeout_ms
            TransportProtocol::Tcp,
            logging,
        )
        .map_err(|e| eyre!("Failed to generate testnet configuration: {:?}", e))
    }

    fn setup_assets_directory(&self, home_dir: &Path) -> Result<()> {
        let assets_dir = home_dir.join("assets");
        fs::create_dir_all(&assets_dir)?;

        // Copy or create JWT secret
        let jwt_source = PathBuf::from("./assets/jwtsecret");
        let jwt_dest = assets_dir.join("jwtsecret");

        if jwt_source.exists() {
            // Copy existing jwtsecret from project
            fs::copy(&jwt_source, &jwt_dest).context("Failed to copy jwtsecret")?;
        } else {
            // Generate a new JWT secret (32 random hex bytes)
            use std::io::Write;
            let secret: [u8; 32] = rand::random();
            let hex_secret = hex::encode(secret);
            let mut file = fs::File::create(&jwt_dest)?;
            file.write_all(hex_secret.as_bytes())?;
        }

        Ok(())
    }

    fn generate_emerald_configs(&self, home_dir: &Path) -> Result<()> {
        use super::types::RethPorts;

        for i in 0..self.nodes {
            let config_dir = home_dir.join(i.to_string()).join("config");
            fs::create_dir_all(&config_dir)?;

            let config_path = config_dir.join("emerald.toml");
            let ports = RethPorts::for_node(i);

            // JWT secret is in the assets directory
            let jwt_path = home_dir.join("assets").join("jwtsecret");

            // Create Emerald config
            let config_content = format!(
                r#"moniker = "node-{}"
execution_authrpc_address = "http://localhost:{}"
engine_authrpc_address = "http://localhost:{}"
jwt_token_path = "{}"
sync_timeout_ms = 120000
sync_initial_delay_ms = 100
el_node_type = "archive"
min_block_time = "500ms"
"#,
                i,
                ports.http,    // execution RPC port
                ports.authrpc, // engine auth RPC port
                jwt_path.display(),
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
                "Using `{}` for Emerald binary when extracting public keys",
                emerald_bin.display()
            );

            let output = Command::new(emerald_bin)
                .args(["show-pubkey"])
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

        // Create assets directory inside home_dir
        let assets_dir = home_dir.join("assets");
        fs::create_dir_all(&assets_dir)?;

        let genesis_output = assets_dir.join("genesis.json");
        let emerald_genesis_output = assets_dir.join("emerald_genesis.json");

        // Check for emerald-utils binary
        let emerald_utils_bin = {
            let p = PathBuf::from(self.emerald_utils_bin.clone());
            if p.exists() {
                p
            } else {
                PathBuf::from("emerald-utils")
            }
        };
        println!(
            "  Using emerald-utils from: {}",
            emerald_utils_bin.display()
        );

        let output = Command::new(emerald_utils_bin.clone())
            .args(["genesis", "--public-keys-file"])
            .arg(&pubkeys_file)
            .args([
                "--poa-owner-address",
                "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
                "--evm-genesis-output",
            ])
            .arg(&genesis_output)
            .args(["--emerald-genesis-output"])
            .arg(&emerald_genesis_output)
            .output()
            .with_context(|| {
                format!(
                    "Failed to execute emerald-utils. Tried:\n  \
                     1. ./target/debug/emerald-utils ({})\n  \
                     2. emerald-utils in PATH\n\n\
                     Please ensure emerald-utils is built or available in PATH.\n\
                     Run: cargo build --bin emerald-utils",
                    if emerald_utils_bin.exists() {
                        "found"
                    } else {
                        "not found"
                    }
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(eyre!(
                "emerald-utils genesis command failed:\n\nSTDERR:\n{}\n\nSTDOUT:\n{}",
                stderr,
                stdout
            ));
        }

        Ok(())
    }

    fn spawn_reth_nodes(&self, home_dir: &Path) -> Result<Vec<RethProcess>> {
        let assets_dir = home_dir.join("assets");
        let mut processes = Vec::new();

        for i in 0..self.nodes {
            let reth_node = RethNode::new(
                i,
                home_dir.to_path_buf(),
                assets_dir.clone(),
                &self.reth_config_path,
            );
            print!("  Starting Reth node {i}... ");
            let process = reth_node.spawn(&self.custom_reth_bin)?;
            println!("✓ (PID: {})", process.pid);
            processes.push(process);

            // Small delay between spawns
            std::thread::sleep(core::time::Duration::from_millis(500));
        }

        Ok(processes)
    }

    fn wait_for_reth_nodes(&self, home_dir: &Path) -> Result<()> {
        let assets_dir = home_dir.join("assets");

        for i in 0..self.nodes {
            let reth_node = RethNode::new(
                i,
                home_dir.to_path_buf(),
                assets_dir.clone(),
                &self.reth_config_path,
            );
            print!("  Waiting for Reth node {i} to be ready... ");
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
            println!("✓");
        }

        Ok(())
    }

    fn connect_reth_peers(&self, home_dir: &Path) -> Result<()> {
        let assets_dir = home_dir.join("assets");
        let mut enodes = Vec::new();

        // Get all enodes
        for i in 0..self.nodes {
            let reth_node = RethNode::new(
                i,
                home_dir.to_path_buf(),
                assets_dir.clone(),
                &self.reth_config_path,
            );
            print!("  Getting enode for Reth node {i}... ");
            let enode = reth_node.get_enode()?;
            println!("✓");
            enodes.push(enode);
        }

        // Connect each node to all other nodes
        for i in 0..self.nodes {
            let reth_node = RethNode::new(
                i,
                home_dir.to_path_buf(),
                assets_dir.clone(),
                &self.reth_config_path,
            );
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
            let process = self.spawn_emerald_node(i, home_dir)?;
            println!("✓ (PID: {})", process.pid);
            processes.push(process);

            // Small delay between spawns
            std::thread::sleep(core::time::Duration::from_millis(500));
        }

        Ok(processes)
    }

    fn spawn_emerald_node(&self, node_id: usize, home_dir: &Path) -> Result<EmeraldProcess> {
        let node_home = home_dir.join(node_id.to_string());
        let config_file = node_home.join("config").join("emerald.toml");

        // Create logs directory
        let log_dir = node_home.join("logs");
        fs::create_dir_all(&log_dir)?;

        let log_file_path = log_dir.join("emerald.log");
        let pid_file = node_home.join("emerald.pid");

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
            "Using `{}` for Emerald binary to spawn node",
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
        std::thread::sleep(core::time::Duration::from_millis(100));

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
