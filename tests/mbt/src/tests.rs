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
    spec = "../../specs/emerald_app.qnt",
    max_samples = 10,
    max_steps = 128
)]
fn simulation_no_crashes() -> impl Driver {
    EmeraldDriver::default()
}

#[quint_run(
    spec = "../../specs/emerald_app.qnt",
    step = "step_with_crashes",
    max_samples = 1,
    max_steps = 64
)]
#[ignore]
fn simulation_with_crashes() -> impl Driver {
    EmeraldDriver::default()
}
