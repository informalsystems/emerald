mod driver;
mod history;
mod reth;
mod state;
mod sut;

#[cfg(test)]
mod tests;

// NOTE: We export the primary MBT struct so that Rust can consider any code not
// referenced by the test driver as dead code.
pub use driver::EmeraldDriver;

// Node identifiers. They must match the `emerald_mbt.qnt` and
// `emerald_tests.qnt` specifications.
const NODES: [&str; 3] = ["node1", "node2", "node3"];
