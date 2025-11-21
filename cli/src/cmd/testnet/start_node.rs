//! Start a specific node in the testnet

use core::time::Duration;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use color_eyre::eyre::{eyre, Context as _};
use color_eyre::Result;

use super::reth;
use super::types::RethNode;

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct TestnetStartNodeCmd {
    /// Node ID to start
    pub node_id: usize,
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

        println!("🚀 Starting node {}...", self.node_id);

        // Check if custom-reth is available
        print!("Checking custom-reth installation... ");
        match reth::check_installation() {
            Ok(version) => {
                println!("✓ {}", version.lines().next().unwrap_or(&version));
            }
            Err(e) => {
                println!("✗");
                return Err(e.wrap_err(
                    "Custom reth is not available. Make sure custom-reth/ directory exists and contains a valid reth binary or custom-reth binary is in your $PATH."
                ));
            }
        }

        // Start Reth process
        println!("\n🔗 Starting Reth execution client...");
        let assets_dir = home_dir.join("assets");
        let reth_node = RethNode::new(self.node_id, home_dir.to_path_buf(), assets_dir);
        let reth_process = reth_node.spawn()?;
        println!("✓ Reth node started (PID: {})", reth_process.pid);

        // Wait for Reth to be ready
        println!("\n⏳ Waiting for Reth node to initialize...");
        reth_node.wait_for_ready(30)?;
        println!("✓ Reth node ready");

        // Connect to existing peers
        println!("\n🔗 Connecting to existing peers...");
        self.connect_to_peers(home_dir, self.node_id)?;
        println!("✓ Connected to peers");

        // Start Emerald process
        println!("\n💎 Starting Emerald consensus node...");
        let emerald_process = self.spawn_emerald_node(home_dir, self.node_id)?;
        println!("✓ Emerald node started (PID: {})", emerald_process.pid);

        println!("\n✅ Node {} started successfully!", self.node_id);
        println!("\n📁 Logs:");
        println!(
            "  Reth: {}/{}/logs/reth.log",
            home_dir.display(),
            self.node_id
        );
        println!(
            "  Emerald: {}/{}/logs/emerald.log",
            home_dir.display(),
            self.node_id
        );

        Ok(())
    }

    fn connect_to_peers(&self, home_dir: &Path, node_id: usize) -> Result<()> {
        let assets_dir = home_dir.join("assets");
        let node = RethNode::new(node_id, home_dir.to_path_buf(), assets_dir.clone());

        // Find all existing nodes and get their enodes
        let mut connected = 0;
        for entry in fs::read_dir(home_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if let Ok(id) = name.parse::<usize>() {
                        if id != node_id {
                            let peer_node =
                                RethNode::new(id, home_dir.to_path_buf(), assets_dir.clone());
                            // Try to get enode and connect
                            if let Ok(enode) = peer_node.get_enode() {
                                print!("  Connecting to node {id}... ");
                                if node.add_peer(&enode).is_ok() {
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

        // Create logs directory if it doesn't exist
        let log_dir = node_home.join("logs");
        fs::create_dir_all(&log_dir)?;

        let log_file_path = log_dir.join("emerald.log");
        let pid_file = node_home.join("emerald.pid");

        // Check for built binary first, then fallback to PATH
        let debug_binary = std::path::Path::new("./target/debug/emerald");
        let cmd = if debug_binary.exists() {
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
