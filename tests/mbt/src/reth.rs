use std::process::Command;

use anyhow::{bail, Result};

use crate::NODES;

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
    run_reth_make_cmd(&format!("reth{node_idx}"), "testnet-reth-recreate")
}

pub fn restart(node_idx: usize) -> Result<()> {
    run_reth_make_cmd(&format!("reth{node_idx}"), "testnet-reth-restart")
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
