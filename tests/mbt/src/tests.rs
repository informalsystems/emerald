use quint_connect::{quint_run, quint_test};

use crate::driver::EmeraldDriver;

#[quint_test(
    spec = "../../specs/emerald_tests.qnt",
    test = "emeraldSingleHeightConsensusTest",
    max_samples = 1
)]
fn test_single_height_consensus() -> impl Driver {
    EmeraldDriver::default()
}

#[quint_test(
    spec = "../../specs/emerald_tests.qnt",
    test = "emeraldTimeoutAndDecideTest",
    max_samples = 1
)]
fn test_timeout_and_decide() -> impl Driver {
    EmeraldDriver::default()
}

#[quint_test(
    spec = "../../specs/emerald_tests.qnt",
    test = "emeraldStalledNodeCatchesUpViaSyncTest",
    max_samples = 1
)]
fn test_stalled_node_catches_up_via_sync() -> impl Driver {
    EmeraldDriver::default()
}

#[quint_test(
    spec = "../../specs/emerald_tests.qnt",
    test = "emeraldStalledNodeCatchesUpAfterCrashTest",
    max_samples = 1
)]
fn test_stalled_node_catches_up_after_crash() -> impl Driver {
    EmeraldDriver::default()
}

#[quint_test(
    spec = "../../specs/emerald_tests.qnt",
    test = "emeraldStalledNodeCatchesUpAfterRestartTest",
    max_samples = 1
)]
fn test_stalled_node_catches_up_after_restart() -> impl Driver {
    EmeraldDriver::default()
}

#[quint_run(
    spec = "../../specs/emerald_mbt.qnt",
    step = "step_no_failures",
    max_samples = 32,
    max_steps = 128
)]
fn simulation_no_failures() -> impl Driver {
    EmeraldDriver::default()
}

#[quint_run(
    spec = "../../specs/emerald_mbt.qnt",
    step = "step_with_failures",
    max_samples = 32,
    max_steps = 128
)]
fn simulation_with_failures() -> impl Driver {
    EmeraldDriver::default()
}
