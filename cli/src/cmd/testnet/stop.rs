//! Stop all nodes in the testnet

use std::fs;
use std::path::Path;
use std::process::Command;

use clap::Parser;
use color_eyre::Result;
use tracing::{debug, info, warn};

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct TestnetStopCmd {}

impl TestnetStopCmd {
    /// Execute the stop command
    pub fn run(&self, home_dir: &Path) -> Result<()> {
        info!("Stopping all testnet nodes");

        if !home_dir.exists() {
            warn!(
                "Testnet directory does not exist at {}",
                home_dir.display()
            );
            return Ok(());
        }

        let mut stopped_count = 0;
        let mut total_processes = 0;

        // Extract information required to stop nodes
        let node_infos = fs::read_dir(home_dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .filter_map(|path| {
                let node_id = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|s| s.parse::<usize>().ok())?;
                Some((node_id, path))
            });

        // Iterate through all node directories
        for (node_id, path) in node_infos {
            info!("Stopping node {node_id}");

            // Stop Reth process
            let reth_pid_file = path.join("reth.pid");
            if reth_pid_file.exists() {
                if let Ok(pid_str) = fs::read_to_string(&reth_pid_file) {
                    if let Ok(pid) = pid_str.trim().parse::<u32>() {
                        total_processes += 1;
                        debug!("Stopping Reth (PID: {pid})");
                        match Command::new("kill").args(["-9", &pid.to_string()]).output() {
                            Ok(output) if output.status.success() => {
                                info!("Stopped Reth (PID: {pid})");
                                stopped_count += 1;
                            }
                            _ => {
                                debug!("Process may already be stopped (PID: {pid})");
                            }
                        }
                        let _ = fs::remove_file(&reth_pid_file);
                    }
                }
            }

            // Stop Emerald process
            let emerald_pid_file = path.join("emerald.pid");
            if emerald_pid_file.exists() {
                if let Ok(pid_str) = fs::read_to_string(&emerald_pid_file) {
                    if let Ok(pid) = pid_str.trim().parse::<u32>() {
                        total_processes += 1;
                        debug!("Stopping Emerald (PID: {pid})");
                        match Command::new("kill").args(["-9", &pid.to_string()]).output() {
                            Ok(output) if output.status.success() => {
                                info!("Stopped Emerald (PID: {pid})");
                                stopped_count += 1;
                            }
                            _ => {
                                debug!("Process may already be stopped (PID: {pid})");
                            }
                        }
                        let _ = fs::remove_file(&emerald_pid_file);
                    }
                }
            }
        }

        if total_processes == 0 {
            warn!("No running processes found");
        } else {
            info!("Stopped {stopped_count}/{total_processes} processes");
        }

        Ok(())
    }
}
