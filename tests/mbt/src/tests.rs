use quint_connect::{quint_run, quint_test};

use crate::driver::EmeraldDriver;

/// Happy path: all 3 nodes start from genesis, the proposer (node1) creates a
/// proposal, all nodes receive it and decide at height 1.
#[quint_test(
    spec = "../../specs/emerald_tests.qnt",
    test = "emeraldSingleHeightConsensusTest",
    max_samples = 1
)]
fn test_single_height_consensus() -> impl Driver {
    EmeraldDriver::default()
}

/// Timeout recovery: node1 proposes but all nodes timeout at round 0, advance
/// to round 1 where node2 proposes, and all nodes successfully decide.
#[quint_test(
    spec = "../../specs/emerald_tests.qnt",
    test = "emeraldTimeoutAndDecideTest",
    max_samples = 1
)]
fn test_timeout_and_decide() -> impl Driver {
    EmeraldDriver::default()
}

/// Sync catch-up: nodes 1 and 2 decide on heights 1 and 2 while node3 is
/// stalled; node3 catches up by processing synced values for both heights.
#[quint_test(
    spec = "../../specs/emerald_tests.qnt",
    test = "emeraldStalledNodeCatchesUpViaSyncTest",
    max_samples = 1
)]
fn test_stalled_node_catches_up_via_sync() -> impl Driver {
    EmeraldDriver::default()
}

/// Crash recovery: same as sync catch-up but node3 crashes first, restarts from
/// genesis (losing all state), then catches up via sync.
#[quint_test(
    spec = "../../specs/emerald_tests.qnt",
    test = "emeraldStalledNodeCatchesUpAfterCrashTest",
    max_samples = 1
)]
fn test_stalled_node_catches_up_after_crash() -> impl Driver {
    EmeraldDriver::default()
}

/// Restart recovery: node3 syncs and decides height 1, then does a process
/// restart (preserving decided state), and catches up on height 2 via sync.
#[quint_test(
    spec = "../../specs/emerald_tests.qnt",
    test = "emeraldStalledNodeCatchesUpAfterRestartTest",
    max_samples = 1
)]
fn test_stalled_node_catches_up_after_restart() -> impl Driver {
    EmeraldDriver::default()
}

/// Random exploration without any failures.
#[quint_run(
    spec = "../../specs/emerald_mbt.qnt",
    step = "step_no_failures",
    max_samples = 32,
    max_steps = 128
)]
fn simulation_no_failures() -> impl Driver {
    EmeraldDriver::default()
}

/// Random exploration allowing at most one failure per (node, height) pair.
#[quint_run(
    spec = "../../specs/emerald_mbt.qnt",
    step = "step_with_failures",
    max_samples = 32,
    max_steps = 128
)]
fn simulation_with_failures() -> impl Driver {
    EmeraldDriver::default()
}
