use std::path::PathBuf;
use std::process::Command;

use anyhow::{ensure, Result};

use crate::NODES;

/// Recreate all Reth instances using the project's testnet commands.
///
/// Note that reth instance names are composed based on the [NODES] list. If
/// there are 3 nodes in [NODES], Reth instances will be `reth0 reth1 reth2`,
/// for example.
pub fn recreate_all() -> Result<()> {
    let mut nodes = String::new();
    for i in 0..NODES.len() {
        nodes.push_str("reth");
        nodes.push_str(&i.to_string());
        nodes.push(' ');
    }
    run_reth_make_cmd(&nodes, "testnet-reth-recreate")
}

/// Recreate the given Reth instance using the project's testnet commands.
///
/// The given node index becomes the Reth identifier in the testnet config. If 0
/// is given, the `reth0` instance will be recreated, for example.
pub fn recreate(node_idx: usize) -> Result<()> {
    run_reth_make_cmd(&format!("reth{node_idx}"), "testnet-reth-recreate")
}

/// Restart the given Reth instance using the project's testnet commands.
///
/// The given node index becomes the Reth identifier in the testnet config. If 0
/// is given, the `reth0` instance will be restarted, for example.
pub fn restart(node_idx: usize) -> Result<()> {
    run_reth_make_cmd(&format!("reth{node_idx}"), "testnet-reth-restart")
}

fn run_reth_make_cmd(nodes: &str, cmd: &str) -> Result<()> {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;

    let res = Command::new("make")
        .env("RETH_NODES", nodes)
        .arg(cmd)
        .current_dir(project_root)
        .output()?;

    ensure!(
        res.status.success(),
        "Failed to run make command: {}",
        String::from_utf8_lossy(&res.stderr),
    );

    Ok(())
}
