use quint_connect::quint_test;

use crate::driver::EmeraldDriver;

/// Test Scenario 2: Two Heights (controlled sequence)
/// From spec line 811-827
#[quint_test(spec = "../../specs/emerald_app.qnt", test = "twoHeightConsensus")]
fn two_height_consensus() -> impl Driver {
    EmeraldDriver::default()
}
