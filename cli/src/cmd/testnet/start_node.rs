//! Start a specific node in the testnet

use core::time::Duration;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use color_eyre::eyre::{eyre, Context as _};
use color_eyre::Result;
use tracing::{debug, info, warn};

use super::reth;
use super::types::RethNode;
use crate::cmd::testnet::rpc::RpcClient;
use crate::cmd::testnet::utils::status::{is_node_running, NodeStatus};
use crate::cmd::testnet::ProcessHandle;
use crate::utils::retry::retry_with_timeout;

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct TestnetStartNodeCmd {
    /// Node ID to start
    pub node_id: usize,

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
}

impl TestnetStartNodeCmd {
    /// Execute the start-node command
    pub fn run(&self, home_dir: &Path) -> Result<()> {
        let node_home = home_dir.join(self.node_id.to_string());

        if !node_home.exists() {
            return Err(eyre!(
                "Node {} does not exist at {}",
                self.node_id,
                node_home.display()
            ));
        }

        // Early return if the node is already running
        let node_dir = home_dir.join(self.node_id.to_string());
        let emerald_pid_file = node_dir.join("emerald.pid");
        let emerald_status = if emerald_pid_file.exists() {
            match ProcessHandle::from_pid_file(&emerald_pid_file) {
                Ok(handle) => is_node_running(&handle),
                Err(e) => {
                    info!(
                        "Process handle for node `{}` not found due to {e}. Will try to start it",
                        self.node_id
                    );
                    NodeStatus::Uninitialised
                }
            }
        } else {
            NodeStatus::Uninitialised
        };

        let emerald_pid_file = node_dir.join("reth.pid");
        let reth_status = if emerald_pid_file.exists() {
            match ProcessHandle::from_pid_file(&emerald_pid_file) {
                Ok(handle) => is_node_running(&handle),
                Err(e) => {
                    info!(
                        "Process handle for node `{}` not found due to {e}. Will try to start it",
                        self.node_id
                    );
                    NodeStatus::Uninitialised
                }
            }
        } else {
            NodeStatus::Uninitialised
        };

        if (emerald_status, reth_status) == (NodeStatus::Running, NodeStatus::Running) {
            info!("Emerald and Reth nodes are already running");
            return Ok(());
        }

        info!("Starting node {}", self.node_id);

        // Check if custom-reth is available
        debug!("Checking custom-reth installation");
        match reth::check_installation(&self.custom_reth_bin) {
            Ok(version) => {
                info!(
                    "Custom-reth installation verified: {}",
                    version.lines().next().unwrap_or(&version)
                );
            }
            Err(e) => {
                warn!("Custom-reth installation check failed");
                return Err(e.wrap_err(
                    "Custom reth is not available. Make sure custom-reth/ directory exists and contains a valid reth binary or custom-reth binary is in your $PATH."
                ));
            }
        }

        // Start Reth process
        if reth_status != NodeStatus::Running {
            info!("Starting Reth execution client");
            let assets_dir = home_dir.join("assets");
            let reth_node = RethNode::new(
                self.node_id,
                home_dir.to_path_buf(),
                assets_dir,
                &self.reth_config_path,
            );
            let reth_process = reth_node.spawn(&self.custom_reth_bin)?;
            info!("Reth node started (PID: {})", reth_process.pid);

            // Wait for Reth to be ready
            info!("Waiting for Reth node to initialize");
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
            info!("Reth node ready");

            // Connect to existing peers
            info!("Connecting to existing peers");
            self.connect_to_peers(home_dir, self.node_id)?;
            info!("Connected to peers");
        }

        // Start Emerald process
        if emerald_status != NodeStatus::Running {
            info!("Starting Emerald consensus node");
            let emerald_process = self.spawn_emerald_node(home_dir, self.node_id)?;
            info!("Emerald node started (PID: {})", emerald_process.pid);

            info!("Node {} started successfully!", self.node_id);
            info!("Logs:");
            info!(
                "  Reth: {}/{}/logs/reth.log",
                home_dir.display(),
                self.node_id
            );
            info!(
                "  Emerald: {}/{}/logs/emerald.log",
                home_dir.display(),
                self.node_id
            );
        }

        Ok(())
    }

    fn connect_to_peers(&self, home_dir: &Path, node_id: usize) -> Result<()> {
        let assets_dir = home_dir.join("assets");
        let node = RethNode::new(
            node_id,
            home_dir.to_path_buf(),
            assets_dir.clone(),
            &self.reth_config_path,
        );

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

        // Find all existing nodes and get their enodes
        let mut connected = 0;
        for id in ids {
            let peer_node = RethNode::new(
                id,
                home_dir.to_path_buf(),
                assets_dir.clone(),
                &self.reth_config_path,
            );
            // Try to get enode and connect
            if let Ok(enode) = peer_node.get_enode() {
                debug!("Connecting to node {id}");
                if node.add_peer(&enode).is_ok() {
                    debug!("Connected to node {id}");
                    connected += 1;
                } else {
                    debug!("Failed to connect to node {id} (skipped)");
                }
            }
        }

        if connected == 0 {
            warn!("No existing peers found to connect to");
        }

        Ok(())
    }

    fn spawn_emerald_node(&self, home_dir: &Path, node_id: usize) -> Result<EmeraldProcess> {
        let node_home = home_dir.join(node_id.to_string());
        let config_file = node_home.join("config").join("emerald.toml");

        // Create logs directory if it doesn't exist
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
