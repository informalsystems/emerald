use quint_connect::quint_test;

use crate::driver::EmeraldDriver;

#[quint_test(
    spec = "../../specs/emerald_tests.qnt",
    test = "emeraldSingleHeightConsensusTest"
)]
fn sigle_height_consensus() -> impl Driver {
    EmeraldDriver::default()
}
