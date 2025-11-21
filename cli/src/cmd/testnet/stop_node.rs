//! Stop a specific node in the testnet

use std::fs;
use std::path::Path;
use std::process::Command;

use clap::Parser;
use color_eyre::eyre::eyre;
use color_eyre::Result;

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct TestnetStopNodeCmd {
    /// Node ID to stop
    pub node_id: usize,
}

impl TestnetStopNodeCmd {
    /// Execute the stop-node command
    pub fn run(&self, home_dir: &Path) -> Result<()> {
        let node_home = home_dir.join(self.node_id.to_string());

        if !node_home.exists() {
            return Err(eyre!("Node {} does not exist at {}", self.node_id, node_home.display()));
        }

        println!("🛑 Stopping node {}...", self.node_id);

        let mut stopped_count = 0;

        // Stop Reth process
        let reth_pid_file = node_home.join("reth.pid");
        if reth_pid_file.exists() {
            match fs::read_to_string(&reth_pid_file) {
                Ok(pid_str) => {
                    if let Ok(pid) = pid_str.trim().parse::<u32>() {
                        print!("  Stopping Reth process (PID: {})... ", pid);
                        match Command::new("kill").args(["-9", &pid.to_string()]).output() {
                            Ok(output) if output.status.success() => {
                                println!("✓");
                                stopped_count += 1;
                            }
                            _ => {
                                println!("✗ (failed to stop)");
                            }
                        }
                        // Always remove PID file regardless
                        let _ = fs::remove_file(&reth_pid_file);
                    }
                }
                Err(_) => println!("  No Reth PID file found"),
            }
        } else {
            println!("  No Reth PID file found");
        }

        // Stop Emerald process
        let emerald_pid_file = node_home.join("emerald.pid");
        if emerald_pid_file.exists() {
            match fs::read_to_string(&emerald_pid_file) {
                Ok(pid_str) => {
                    if let Ok(pid) = pid_str.trim().parse::<u32>() {
                        print!("  Stopping Emerald process (PID: {})... ", pid);
                        match Command::new("kill").args(["-9", &pid.to_string()]).output() {
                            Ok(output) if output.status.success() => {
                                println!("✓");
                                stopped_count += 1;
                            }
                            _ => {
                                println!("✗ (failed to stop)");
                            }
                        }
                        // Always remove PID file regardless
                        let _ = fs::remove_file(&emerald_pid_file);
                    }
                }
                Err(_) => println!("  No Emerald PID file found"),
            }
        } else {
            println!("  No Emerald PID file found");
        }

        if stopped_count == 0 {
            println!("\n⚠️  No running processes found for node {}", self.node_id);
        } else {
            println!("\n✅ Stopped {} process(es) for node {}", stopped_count, self.node_id);
        }

        Ok(())
    }
}
