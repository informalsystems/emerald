mod driver;
mod history;
mod reth;
mod state;
mod sut;

#[cfg(test)]
mod tests;

pub use driver::EmeraldDriver;

// Must match spec's node identifiers.
const NODES: [&str; 3] = ["node1", "node2", "node3"];
