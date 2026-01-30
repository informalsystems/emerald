//! Shared types for testnet commands

use core::time::Duration;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use color_eyre::Result;

use crate::cmd::testnet::config::reth_config::RethNodeConfig;

/// Handle for a running process
#[derive(Debug, Clone)]
pub struct ProcessHandle {
    pub pid: u32,
    pub name: String,
}

impl ProcessHandle {
    /// Read PID from file
    pub fn from_pid_file(path: &Path) -> Result<Self> {
        let pid_str = std::fs::read_to_string(path)?;
        let pid = pid_str.trim().parse()?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        Ok(Self { pid, name })
    }

    /// Check if process is running
    pub fn is_running(&self) -> bool {
        #[cfg(unix)]
        {
            use std::process::Command;
            Command::new("kill")
                .args(["-0", &self.pid.to_string()])
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
        }
        #[cfg(not(unix))]
        {
            // TODO: non-unix support
            false
        }
    }

    /// Stop process gracefully (SIGTERM -> wait -> SIGKILL if needed)
    pub fn stop(&self, timeout: Duration) -> Result<()> {
        #[cfg(unix)]
        {
            use std::process::Command;
            use std::thread::sleep;
            use std::time::Instant;

            // Try SIGTERM first
            Command::new("kill")
                .args(["-TERM", &self.pid.to_string()])
                .output()?;

            // Wait for process to exit
            let start = Instant::now();
            while self.is_running() && start.elapsed() < timeout {
                sleep(Duration::from_millis(100));
            }

            // If still running, force kill
            if self.is_running() {
                Command::new("kill")
                    .args(["-KILL", &self.pid.to_string()])
                    .output()?;
            }

            Ok(())
        }
        #[cfg(not(unix))]
        {
            // TODO: non-unix support
            Ok(())
        }
    }

    /// Write PID to file
    pub fn write_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.pid.to_string())?;
        Ok(())
    }
}

/// Metadata about the testnet
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestnetMetadata {
    pub num_nodes: usize,
    pub created_at: SystemTime,
    pub genesis_hash: String,
}

impl TestnetMetadata {
    /// Load from nodes directory
    pub fn load(home_dir: &Path) -> Result<Self> {
        let metadata_file = home_dir.join("testnet_metadata.json");
        let contents = std::fs::read_to_string(metadata_file)?;
        let metadata = serde_json::from_str(&contents)?;
        Ok(metadata)
    }

    /// Save to nodes directory
    pub fn save(&self, home_dir: &Path) -> Result<()> {
        let metadata_file = home_dir.join("testnet_metadata.json");
        if let Some(parent) = metadata_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string_pretty(&self)?;
        std::fs::write(metadata_file, contents)?;
        Ok(())
    }
}

/// Reth port configuration for a node
#[derive(Debug, Clone, Copy)]
pub struct RethPorts {
    pub http: u16,
    pub ws: u16,
    pub authrpc: u16,
    pub metrics: u16,
    pub discovery: u16,
    pub p2p: u16,
}

impl RethPorts {
    /// Calculate ports for a given node index
    /// Each node gets 10 consecutive ports starting from base
    pub fn for_node(node_id: usize) -> Self {
        let base = 8645 + (node_id * 30);
        Self {
            http: base as u16,            // 8545, 8555, 8565, ...
            ws: (base + 1) as u16,        // 8546, 8556, 8566, ...
            authrpc: (base + 2) as u16,   // 8547, 8557, 8567, ...
            metrics: (base + 3) as u16,   // 8548, 8558, 8568, ...
            discovery: (base + 4) as u16, // 8549, 8559, 8569, ...
            p2p: (base + 4) as u16,       // 8549, 8559, 8569, ... (same as discovery)
        }
    }
}

/// Reth node
#[derive(Debug, Clone)]
pub struct RethNode {
    pub node_id: usize,
    pub home_dir: PathBuf,
    pub data_dir: PathBuf,
    pub genesis_file: PathBuf,
    pub jwt_secret: PathBuf,
    pub ports: RethPorts,
    pub config: RethNodeConfig,
}

impl RethNode {
    /// Create a new Reth node configuration
    pub fn new(
        node_id: usize,
        home_dir: PathBuf,
        assets_dir: PathBuf,
        config_path: &Option<PathBuf>,
    ) -> Self {
        let data_dir = home_dir.join(node_id.to_string()).join("reth-data");
        let genesis_file = assets_dir.join("genesis.json");
        let jwt_secret = assets_dir.join("jwtsecret");
        let ports = RethPorts::for_node(node_id);

        let config = match config_path {
            None => {
                // Case 1: no config file → pure defaults
                RethNodeConfig::default()
            }
            Some(path) => {
                let contents = std::fs::read_to_string(path)
                    .unwrap_or_else(|_| panic!("failed to read config file {path:?}"));
                let cfg: RethNodeConfig = toml::from_str(&contents)
                    .unwrap_or_else(|_| panic!("failed to parse config file {path:?}"));
                cfg
            }
        };

        Self {
            node_id,
            home_dir,
            data_dir,
            genesis_file,
            jwt_secret,
            ports,
            config,
        }
    }
}
