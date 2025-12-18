//! Remove all testnet data

use std::fs;
use std::path::Path;
use std::process::Command;

use clap::Parser;
use color_eyre::eyre::eyre;
use color_eyre::Result;
use tracing::{debug, info, warn};

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct TestnetDestroyCmd {
    /// Skip confirmation prompt
    #[clap(long, short)]
    pub force: bool,
}

impl TestnetDestroyCmd {
    /// Execute the destroy command
    pub fn run(&self, home_dir: &Path) -> Result<()> {
        if !home_dir.exists() {
            warn!(
                "Testnet directory does not exist at {}",
                home_dir.display()
            );
            return Ok(());
        }

        // Confirm with user unless --force is specified
        if !self.force {
            warn!("This will stop all nodes and permanently delete all testnet data at:");
            info!("   {}", home_dir.display());
            info!("");
            info!("   Are you sure? (y/N): ");

            use std::io::{self, Write};
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            let input = input.trim().to_lowercase();
            if input != "y" && input != "yes" {
                info!("Cancelled.");
                return Ok(());
            }
        }

        // First, stop all running processes
        info!("Stopping all running nodes");
        self.stop_all_nodes(home_dir)?;

        info!("Removing testnet data");

        // Remove the entire directory
        fs::remove_dir_all(home_dir).map_err(|e| eyre!("Failed to remove directory: {}", e))?;

        info!("Testnet data removed successfully");

        Ok(())
    }

    /// Stop all running nodes before removing data
    fn stop_all_nodes(&self, home_dir: &Path) -> Result<()> {
        let mut stopped_count = 0;

        // Iterate through all node directories
        let entries = fs::read_dir(home_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Check if this is a node directory (has a number as name)
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.parse::<usize>().is_ok() {
                        // Stop Reth process
                        let reth_pid_file = path.join("reth.pid");
                        if reth_pid_file.exists() {
                            if let Ok(pid_str) = fs::read_to_string(&reth_pid_file) {
                                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                                    let _ = Command::new("kill")
                                        .args(["-9", &pid.to_string()])
                                        .output();
                                    stopped_count += 1;
                                }
                            }
                        }

                        // Stop Emerald process
                        let emerald_pid_file = path.join("emerald.pid");
                        if emerald_pid_file.exists() {
                            if let Ok(pid_str) = fs::read_to_string(&emerald_pid_file) {
                                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                                    let _ = Command::new("kill")
                                        .args(["-9", &pid.to_string()])
                                        .output();
                                    stopped_count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        if stopped_count > 0 {
            debug!("Stopped {stopped_count} process(es)");
        }

        Ok(())
    }
}
