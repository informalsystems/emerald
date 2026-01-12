use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Result};

use crate::NODES;

const MAX_STARTUP_ATTEMPTS: u32 = 300;
const RPC_TIMEOUT: Duration = Duration::from_secs(1);
const STARTUP_CHECK_INTERVAL: Duration = Duration::from_millis(100);

pub fn recreate_all() -> Result<()> {
    let mut nodes = String::new();
    for i in 0..NODES.len() {
        nodes.push_str("reth");
        nodes.push_str(&i.to_string());
        nodes.push(' ');
    }
    run_reth_make_cmd(&nodes, "testnet-reth-recreate")
}

pub fn recreate(node_idx: usize) -> Result<()> {
    run_reth_make_cmd(&format!("reth{}", node_idx), "testnet-reth-recreate")
}

pub fn restart(node_idx: usize) -> Result<()> {
    run_reth_make_cmd(&format!("reth{}", node_idx), "testnet-reth-restart")
}

fn run_reth_make_cmd(nodes: &str, cmd: &str) -> Result<()> {
    let res = Command::new("make")
        .env("RETH_NODES", nodes)
        .arg(cmd)
        .current_dir("../..")
        .output()?;

    if !res.status.success() {
        bail!(
            "Failed to run make command: {}",
            String::from_utf8_lossy(&res.stderr)
        );
    }

    Ok(())
}
