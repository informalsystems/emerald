mod driver;
mod state;

#[cfg(test)]
mod reth_manager;

#[cfg(test)]
mod tests;

// Must match spec's node identifiers.
pub const NODES: [&str; 3] = ["node1", "node2", "node3"];
pub const N_NODES: usize = NODES.len();
