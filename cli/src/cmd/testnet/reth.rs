//! Reth process management

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use color_eyre::eyre::{eyre, Context as _};
use color_eyre::Result;

use super::rpc::RpcClient;
use super::types::{ProcessHandle, RethNode};

/// Check if reth is installed and return version
pub fn check_installation(use_cargo: bool) -> Result<String> {
    let output = if use_cargo {
        Command::new("cargo")
            .args(["run", "--manifest-path", "custom-reth/Cargo.toml", "--bin", "custom-reth", "--", "--version"])
            .output()
            .context("Failed to execute 'cargo run --bin custom-reth -- --version'. Is custom-reth available?")?
    } else {
        Command::new("reth")
            .arg("--version")
            .output()
            .context("Failed to execute 'reth --version'. Is reth installed?")?
    };

    if !output.status.success() {
        return Err(eyre!("reth command failed"));
    }

    let version = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string();
    Ok(version)
}

impl RethNode {
    /// Build command line arguments for reth
    pub fn build_args(&self) -> Vec<String> {
        vec![
            "node".to_string(),
            "-vvvv".to_string(),
            "-d".to_string(),
            format!("--datadir={}", self.data_dir.display()),
            format!("--chain={}", self.genesis_file.display()),
            "--http".to_string(),
            format!("--http.port={}", self.ports.http),
            "--http.addr=0.0.0.0".to_string(),
            "--http.corsdomain=*".to_string(),
            "--http.api=admin,net,eth,web3,debug,txpool,trace,ots".to_string(),
            "--ws".to_string(),
            format!("--ws.port={}", self.ports.ws),
            "--ws.addr=0.0.0.0".to_string(),
            "--authrpc.addr=0.0.0.0".to_string(),
            format!("--authrpc.port={}", self.ports.authrpc),
            format!("--authrpc.jwtsecret={}", self.jwt_secret.display()),
            format!("--metrics=127.0.0.1:{}", self.ports.metrics),
            format!("--discovery.port={}", self.ports.discovery),
            format!("--port={}", self.ports.p2p),
            "--nat=extip:127.0.0.1".to_string(),
        ]
    }

    /// Spawn reth process
    pub fn spawn(&self, use_cargo: bool) -> Result<RethProcess> {
        // Ensure directories exist
        if let Some(parent) = self.data_dir.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(&self.data_dir)?;

        let log_dir = self.home_dir.join(self.node_id.to_string()).join("logs");
        fs::create_dir_all(&log_dir)?;

        let log_file_path = log_dir.join("reth.log");
        let log_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file_path)
            .context("Failed to open reth log file")?;

        let args = self.build_args();

        println!("Starting Reth node {} on ports:", self.node_id);
        println!("  HTTP: {}", self.ports.http);
        println!("  AuthRPC: {}", self.ports.authrpc);
        println!("  Metrics: {}", self.ports.metrics);
        println!("  P2P: {}", self.ports.p2p);
        println!("  Logs: {}", log_file_path.display());

        let child = if use_cargo {
            let mut cargo_args = vec![
                "run".to_string(),
                "--manifest-path".to_string(),
                "custom-reth/Cargo.toml".to_string(),
                "--bin".to_string(),
                "custom-reth".to_string(),
                "--".to_string(),
            ];
            cargo_args.extend(args);

            Command::new("cargo")
                .args(&cargo_args)
                .stdout(Stdio::from(log_file.try_clone()?))
                .stderr(Stdio::from(log_file))
                .spawn()
                .context("Failed to spawn reth process via cargo")?
        } else {
            Command::new("reth")
                .args(&args)
                .stdout(Stdio::from(log_file.try_clone()?))
                .stderr(Stdio::from(log_file))
                .spawn()
                .context("Failed to spawn reth process")?
        };

        let pid = child.id();
        let handle = ProcessHandle {
            pid,
            name: format!("reth-{}", self.node_id),
        };

        // Write PID to file
        let pid_file = self.home_dir.join(self.node_id.to_string()).join("reth.pid");
        handle.write_to_file(&pid_file)?;

        Ok(RethProcess {
            child,
            pid,
            log_file: log_file_path,
        })
    }

    /// Wait for reth to be ready (RPC responding)
    pub fn wait_for_ready(&self, timeout_secs: u64) -> Result<()> {
        use std::thread::sleep;
        use std::time::Instant;
        use core::time::Duration;

        let start = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);
        let rpc = RpcClient::new(self.ports.http);

        loop {
            if start.elapsed() > timeout {
                return Err(eyre!(
                    "Timeout waiting for Reth node {} to be ready",
                    self.node_id
                ));
            }

            // Try to query block number (should return 0 for genesis)
            if rpc.get_block_number().is_ok() {
                // RPC is responding, node is ready
                return Ok(());
            }

            sleep(Duration::from_millis(500));
        }
    }

    /// Wait for reth to reach a specific block height
    pub fn wait_for_height(&self, height: u64, timeout_secs: u64) -> Result<()> {
        use std::thread::sleep;
        use std::time::Instant;
        use core::time::Duration;

        let start = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);
        let rpc = RpcClient::new(self.ports.http);

        loop {
            if start.elapsed() > timeout {
                return Err(eyre!(
                    "Timeout waiting for Reth node {} to reach height {}",
                    self.node_id,
                    height
                ));
            }

            // Check block number via RPC
            if let Ok(block_num) = rpc.get_block_number() {
                if block_num >= height {
                    return Ok(());
                }
            }

            sleep(Duration::from_millis(500));
        }
    }

    /// Get enode address for this reth node
    pub fn get_enode(&self) -> Result<String> {
        let rpc = RpcClient::new(self.ports.http);
        rpc.get_enode()
    }

    /// Add peer to this reth node
    pub fn add_peer(&self, enode: &str) -> Result<()> {
        let rpc = RpcClient::new(self.ports.http);
        rpc.add_peer(enode)
    }
}

/// Running Reth process
pub struct RethProcess {
    pub child: Child,
    pub pid: u32,
    pub log_file: PathBuf,
}

impl RethProcess {
    /// Check if the process is still running
    pub fn is_running(&self) -> bool {
        ProcessHandle {
            pid: self.pid,
            name: "reth".to_string(),
        }
        .is_running()
    }
}
