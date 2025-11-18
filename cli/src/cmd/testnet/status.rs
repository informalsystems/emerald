//! Testnet status command - Show status of all nodes

use std::path::Path;

use clap::Parser;
use color_eyre::Result;

use super::rpc::RpcClient;
use super::types::{ProcessHandle, RethPorts};

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct TestnetStatusCmd {
    // No additional options needed for now
}

impl TestnetStatusCmd {
    /// Execute the testnet status command
    pub fn run(&self, home_dir: &Path) -> Result<()> {
        println!("📊 Testnet Status");
        println!("Looking for nodes in: {}\n", home_dir.display());

        // Find all node directories
        let mut node_count = 0;
        let mut running_emerald = 0;
        let mut running_reth = 0;

        for i in 0..100 {
            // Check up to 100 nodes
            let node_dir = home_dir.join(i.to_string());
            if !node_dir.exists() {
                if i == 0 {
                    println!("No testnet found. Run 'emerald testnet start' first.");
                    return Ok(());
                }
                break;
            }

            node_count += 1;
            println!("Node {i}:");

            // Check Emerald status
            let emerald_pid_file = node_dir.join("emerald.pid");
            let emerald_status = if emerald_pid_file.exists() {
                match ProcessHandle::from_pid_file(&emerald_pid_file) {
                    Ok(handle) if handle.is_running() => {
                        running_emerald += 1;
                        format!("Running (PID: {})", handle.pid)
                    }
                    _ => "Stopped".to_string(),
                }
            } else {
                "Not started".to_string()
            };
            println!("  Emerald: {emerald_status}");

            // Check Reth status
            let reth_pid_file = node_dir.join("reth.pid");
            let reth_status = if reth_pid_file.exists() {
                match ProcessHandle::from_pid_file(&reth_pid_file) {
                    Ok(handle) if handle.is_running() => {
                        running_reth += 1;
                        format!("Running (PID: {})", handle.pid)
                    }
                    _ => "Stopped".to_string(),
                }
            } else {
                "Not started".to_string()
            };
            println!("  Reth:    {reth_status}");

            // Get block height if Reth is running
            let ports = RethPorts::for_node(i);
            let rpc = RpcClient::new(ports.http);

            if let Ok(height) = rpc.get_block_number() {
                println!("  Height:  {height}");
            }

            // Get peer count if Reth is running
            if let Ok(peers) = rpc.get_peer_count() {
                println!("  Peers:   {peers}");
            }

            println!();
        }

        println!("Summary:");
        println!("  Total nodes:    {node_count}");
        println!("  Emerald running: {running_emerald}/{node_count}");
        println!("  Reth running:    {running_reth}/{node_count}");

        Ok(())
    }
}
