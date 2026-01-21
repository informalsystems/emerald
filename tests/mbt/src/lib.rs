mod driver;
mod history;
mod reth;
mod runtime;
mod state;
mod sut;

pub use driver::EmeraldDriver;

// Node identifiers. They must match the `emerald_mbt.qnt` and
// `emerald_tests.qnt` specifications.
const NODES: [&str; 3] = ["node1", "node2", "node3"];
