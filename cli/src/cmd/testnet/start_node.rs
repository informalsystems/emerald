//! Start a specific node in the testnet

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
            return Err(eyre!("Node {} does not exist at {}", self.node_id, node_home.display()));
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
                    "Custom reth is not available. Make sure custom-reth/ directory exists and contains a valid reth binary."
                ));
            }
        }

        // Start Reth process
        println!("\n🔗 Starting Reth execution client...");
        let assets_dir = PathBuf::from("./assets");
        let reth_node = RethNode::new(self.node_id, home_dir.to_path_buf(), assets_dir);
        let reth_process = reth_node.spawn()?;
        println!("✓ Reth node started (PID: {})", reth_process.pid);

        // Wait for Reth to be ready
        println!("\n⏳ Waiting for Reth node to initialize...");
        reth_node.wait_for_ready(30)?;
        println!("✓ Reth node ready");

        // Start Emerald process
        println!("\n💎 Starting Emerald consensus node...");
        let emerald_process = self.spawn_emerald_node(home_dir, self.node_id)?;
        println!("✓ Emerald node started (PID: {})", emerald_process.pid);

        println!("\n✅ Node {} started successfully!", self.node_id);
        println!("\n📁 Logs:");
        println!("  Reth: {}/{}/logs/reth.log", home_dir.display(), self.node_id);
        println!("  Emerald: {}/{}/logs/emerald.log", home_dir.display(), self.node_id);

        Ok(())
    }

    fn spawn_emerald_node(&self, home_dir: &Path, node_id: usize) -> Result<EmeraldProcess> {
        let node_home = home_dir.join(node_id.to_string());
        let config_file = node_home.join("config").join("emerald.toml");

        // Create logs directory if it doesn't exist
        let log_dir = node_home.join("logs");
        fs::create_dir_all(&log_dir)?;

        let log_file_path = log_dir.join("emerald.log");
        let log_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file_path)?;

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
