//! Stop all nodes in the testnet

use std::fs;
use std::path::Path;
use std::process::Command;

use clap::Parser;
use color_eyre::Result;

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct TestnetStopCmd {}

impl TestnetStopCmd {
    /// Execute the stop command
    pub fn run(&self, home_dir: &Path) -> Result<()> {
        println!("🛑 Stopping all testnet nodes...\n");

        if !home_dir.exists() {
            println!(
                "⚠️  Testnet directory does not exist at {}",
                home_dir.display()
            );
            return Ok(());
        }

        let mut stopped_count = 0;
        let mut total_processes = 0;

        // Iterate through all node directories
        let entries = fs::read_dir(home_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Check if this is a node directory (has a number as name)
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.parse::<usize>().is_ok() {
                        let node_id = name.parse::<usize>().unwrap();
                        println!("Stopping node {node_id}...");

                        // Stop Reth process
                        let reth_pid_file = path.join("reth.pid");
                        if reth_pid_file.exists() {
                            if let Ok(pid_str) = fs::read_to_string(&reth_pid_file) {
                                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                                    total_processes += 1;
                                    print!("  Stopping Reth (PID: {pid})... ");
                                    match Command::new("kill")
                                        .args(["-9", &pid.to_string()])
                                        .output()
                                    {
                                        Ok(output) if output.status.success() => {
                                            println!("✓");
                                            stopped_count += 1;
                                        }
                                        _ => {
                                            println!("✗ (process may already be stopped)");
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
                                    print!("  Stopping Emerald (PID: {pid})... ");
                                    match Command::new("kill")
                                        .args(["-9", &pid.to_string()])
                                        .output()
                                    {
                                        Ok(output) if output.status.success() => {
                                            println!("✓");
                                            stopped_count += 1;
                                        }
                                        _ => {
                                            println!("✗ (process may already be stopped)");
                                        }
                                    }
                                    let _ = fs::remove_file(&emerald_pid_file);
                                }
                            }
                        }
                    }
                }
            }
        }

        println!();
        if total_processes == 0 {
            println!("⚠️  No running processes found");
        } else {
            println!("✅ Stopped {stopped_count}/{total_processes} processes");
        }

        Ok(())
    }
}
