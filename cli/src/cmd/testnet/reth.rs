//! Reth process management

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use color_eyre::eyre::{eyre, Context as _};
use color_eyre::Result;
use tracing::info;

use super::rpc::RpcClient;
use super::types::{ProcessHandle, RethNode};

/// Check if custom-reth is available and return version
pub fn check_installation(custom_reth_bin_str: &str) -> Result<String> {
    // Check for built binary first, then try PATH
    let custom_reth_bin = {
        let p = PathBuf::from(custom_reth_bin_str);
        if p.exists() {
            p
        } else {
            PathBuf::from("custom-reth")
        }
    };

    let output = Command::new(custom_reth_bin)
        .arg("--version")
        .output()
        .context("Failed to execute custom-reth binary")?;

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
            format!("-{}", self.config.log_verbosity),
            "-d".to_string(),
            format!("--datadir={}", self.data_dir.display()),
            format!("--chain={}", self.genesis_file.display()),
            "--http".to_string(),
            format!("--http.port={}", self.ports.http),
            format!("--http.addr={}", self.config.http_addr),
            format!("--http.corsdomain={}", self.config.http_corsdomain),
            format!("--http.api={}", self.config.http_api.join(",")),
            "--ws".to_string(),
            format!("--ws.port={}", self.ports.ws),
            format!("--ws.addr={}", self.config.ws_addr),
            format!("--authrpc.addr={}", self.config.authrpc_addr),
            format!("--authrpc.port={}", self.ports.authrpc),
            format!("--authrpc.jwtsecret={}", self.jwt_secret.display()),
            format!("--metrics=127.0.0.1:{}", self.ports.metrics),
            format!("--discovery.port={}", self.ports.discovery),
            format!("--port={}", self.ports.p2p),
            format!("--nat={}", self.config.nat),
            format!("--tx-propagation-mode={}", self.config.tx_propagation_mode),
            format!(
                "--txpool.pending-max-count={}",
                self.config.txpool_pending_max_count
            ),
            format!(
                "--txpool.pending-max-size={}",
                self.config.txpool_pending_max_size
            ),
            format!(
                "--txpool.queued-max-count={}",
                self.config.txpool_queued_max_count
            ),
            format!(
                "--txpool.queued-max-size={}",
                self.config.txpool_queued_max_size
            ),
            format!(
                "--txpool.max-account-slots={}",
                self.config.txpool_max_account_slots
            ),
            format!(
                "--txpool.max-batch-size={}",
                self.config.txpool_max_batch_size
            ),
            format!("--max-tx-reqs={}", self.config.max_tx_reqs),
            format!("--max-tx-reqs-peer={}", self.config.max_tx_reqs_peer),
            format!("--max-pending-imports={}", self.config.max_pending_imports),
            format!("--builder.gaslimit={}", self.config.builder_gaslimit),
            format!("--txpool.gas-limit={}", self.config.txpool_gas_limit),
            format!("--builder.interval={}", self.config.builder_interval),
            format!("--rpc.gascap={}", self.config.rpc_gascap),
        ]
    }

    /// Spawn reth process using custom-reth binary
    pub fn spawn(&self, custom_reth_bin_str: &str) -> Result<RethProcess> {
        // Ensure directories exist
        if let Some(parent) = self.data_dir.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(&self.data_dir)?;

        let log_dir = self.home_dir.join(self.node_id.to_string()).join("logs");
        fs::create_dir_all(&log_dir)?;

        let log_file_path = log_dir.join("reth.log");

        let args = self.build_args();

        info!("Starting Reth node {} on ports:", self.node_id);
        info!("  HTTP: {}", self.ports.http);
        info!("  AuthRPC: {}", self.ports.authrpc);
        info!("  Metrics: {}", self.ports.metrics);
        info!("  P2P: {}", self.ports.p2p);
        info!("  Logs: {}", log_file_path.display());

        let pid_file = self
            .home_dir
            .join(self.node_id.to_string())
            .join("reth.pid");

        // Check for built binary first, then fallback to PATH
        let custom_reth_bin = {
            let p = PathBuf::from(custom_reth_bin_str);
            if p.exists() {
                p
            } else {
                PathBuf::from("custom-reth")
            }
        };
        let cmd = format!("{} {}", custom_reth_bin.display(), args.join(" "));

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
