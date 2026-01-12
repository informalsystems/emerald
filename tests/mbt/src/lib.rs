mod driver;
mod history;
mod reth;
mod state;
mod sut;

#[cfg(test)]
mod tests;

// Must match spec's node identifiers.
pub const NODES: [&str; 3] = ["node1", "node2", "node3"];
