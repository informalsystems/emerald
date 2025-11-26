//! Reth process management

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use color_eyre::eyre::{eyre, Context as _};
use color_eyre::Result;

use super::rpc::RpcClient;
use super::types::{ProcessHandle, RethNode};

/// Check if custom-reth is available and return version
pub fn check_installation() -> Result<String> {
    // Check for built binary first, then try PATH
    let debug_binary = std::path::Path::new("./custom-reth/target/debug/custom-reth");

    let output = if debug_binary.exists() {
        Command::new(debug_binary)
            .arg("--version")
            .output()
            .context("Failed to execute custom-reth binary")?
    } else {
        // Try custom-reth in PATH
        Command::new("custom-reth")
            .arg("--version")
            .output()
            .context(
                "Failed to execute 'custom-reth'. Not found in ./custom-reth/target/debug/ or PATH",
            )?
    };

    if !output.status.success() {
        return Err(eyre!("custom-reth command failed"));
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
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
            "--tx-propagation-mode=all".to_string(),
            "--txpool.pending-max-count=50000".to_string(),
            "--txpool.pending-max-size=500".to_string(),
            "--txpool.queued-max-count=50000".to_string(),
            "--txpool.queued-max-size=500".to_string(),
            "--txpool.max-account-slots=50000".to_string(),
            "--txpool.max-batch-size=10000".to_string(),
            "--max-tx-reqs=10000".to_string(),
            "--max-tx-reqs-peer=255".to_string(),
            "--max-pending-imports=10000".to_string(),
            "--builder.gaslimit=66000000000".to_string(),
            "--txpool.gas-limit=3000000000".to_string(),
            "--builder.interval=10ms".to_string(),
        ]
    }

    /// Spawn reth process using custom-reth binary
    pub fn spawn(&self) -> Result<RethProcess> {
        // Ensure directories exist
        if let Some(parent) = self.data_dir.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(&self.data_dir)?;

        let log_dir = self.home_dir.join(self.node_id.to_string()).join("logs");
        fs::create_dir_all(&log_dir)?;

        let log_file_path = log_dir.join("reth.log");

        let args = self.build_args();

        println!("Starting Reth node {} on ports:", self.node_id);
        println!("  HTTP: {}", self.ports.http);
        println!("  AuthRPC: {}", self.ports.authrpc);
        println!("  Metrics: {}", self.ports.metrics);
        println!("  P2P: {}", self.ports.p2p);
        println!("  Logs: {}", log_file_path.display());

        let pid_file = self
            .home_dir
            .join(self.node_id.to_string())
            .join("reth.pid");

        // Check for built binary first, then fallback to PATH
        let debug_binary = std::path::Path::new("./custom-reth/target/debug/custom-reth");
        let cmd = if debug_binary.exists() {
            format!("{} {}", debug_binary.display(), args.join(" "))
        } else {
            // Try PATH - will fail at spawn time if not found
            format!("custom-reth {}", args.join(" "))
        };

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
            .context("Failed to spawn custom-reth process")?;

        // Wait a moment for PID file to be written
        std::thread::sleep(core::time::Duration::from_millis(100));

        // Read PID from file
        let pid_str = fs::read_to_string(&pid_file).context("Failed to read PID file")?;
        let pid = pid_str
            .trim()
            .parse::<u32>()
            .context("Failed to parse PID")?;

        Ok(RethProcess {
            pid,
            log_file: log_file_path,
        })
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
