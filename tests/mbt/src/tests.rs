use quint_connect::quint_run;
use quint_connect::quint_test;

use crate::driver::EmeraldDriver;

#[quint_test(
    spec = "../../specs/emerald_tests.qnt",
    test = "emeraldSingleHeightConsensusTest"
)]
fn sigle_height_consensus() -> impl Driver {
    EmeraldDriver::default()
}

// FIXME: this is another instance of
// https://github.com/informalsystems/emerald/issues/100. However, there's an
// workaround where operators can be instructued to kill and wipe out the data
// for reth, in which case the fix for #100 still applies.
#[quint_test(
    spec = "../../specs/emerald_tests.qnt",
    test = "emeraldNodeCrashAfterConsensusTest"
)]
#[should_panic = "Payload ID should be Some!"]
fn node_crash_after_consensus() -> impl Driver {
    EmeraldDriver::default()
}

// FIXME: this breaks due to the same scenario as node_crash_after_consensus.
#[quint_run(spec = "../../specs/emerald_app.qnt")]
#[should_panic = "Payload ID should be Some!"]
fn simulation() -> impl Driver {
    EmeraldDriver::default()
}
