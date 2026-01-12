use quint_connect::{quint_run, quint_test};

use crate::driver::EmeraldDriver;

#[quint_test(
    spec = "../../specs/emerald_tests.qnt",
    test = "emeraldSingleHeightConsensusTest"
)]
fn single_height_consensus() -> impl Driver {
    EmeraldDriver::default()
}

#[quint_test(
    spec = "../../specs/emerald_tests.qnt",
    test = "emeraldNodeCrashAfterConsensusTest"
)]
fn node_crash_after_consensus() -> impl Driver {
    EmeraldDriver::default()
}

#[quint_run(
    spec = "../../specs/emerald_mbt.qnt",
    step = "step_no_failures",
    max_samples = 20,
    max_steps = 128
)]
fn simulation_no_failures() -> impl Driver {
    EmeraldDriver::default()
}

#[quint_run(
    spec = "../../specs/emerald_mbt.qnt",
    max_samples = 10,
    // TODO: increase # of steps
    // max_steps = 128
    max_steps = 64
)]
fn simulation_with_failures() -> impl Driver {
    EmeraldDriver::default()
}
