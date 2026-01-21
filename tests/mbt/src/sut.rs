mod consensus_ready;
mod decided;
mod get_decided;
mod get_value;
mod process_synced_value;
mod receive_proposal;
mod started_round;

use anyhow::{anyhow, Result};
pub use decided::mock_votes;
use emerald::app::process_consensus_message;
use emerald::node::AppRuntime;
use malachitebft_app_channel::AppMsg;
use malachitebft_eth_types::{Address, EmeraldContext};
use tokio::sync::oneshot::Receiver;

use crate::runtime::Runtime;

/// The Emerald system under test (SUT).
///
/// There is one instance of [Sut] per Emerald node in the Quint trace, which is
/// composed of its address, state components, and local runtime.
///
/// Note that SUT implementation is split on different files at the `sut/`
/// directory where each file focus on the translation of one Quint action.
pub struct Sut {
    pub address: Address,
    pub components: AppRuntime,
    // XXX: There is some task being spawned into the current tokio runtime
    // during the StateComponents initialization that is leaking in the sense
    // that dropping the StateComponents struct doesn't cancel the loop task.
    //
    // This comes up in the form of a held lock on Malachite's WAL that prevents
    // us from simulating a node restart by simply dropping the struct and
    // recreating it pointing to the same home dir.
    //
    // Adding a separate runtime per SUT is a workaround so that we can drop
    // both StateComponents and its associated Runtime. Dropping the Tokio
    // runtime also drops the loop tasks that are holding the WAL lock, hence
    // allowing us to simulate process restarts where the same home dir is
    // reused.
    pub runtime: Runtime,
}

impl Sut {
    /// Process an [AppMsg] using Emerald consensus logic.
    ///
    /// This is the primary Emerald entrypoint for testing. For every Quint
    /// action that needs to be replayed into the Emerald instance, the test
    /// code must translate the Quint step into an [AppMsg] and feed it to this
    /// method.
    async fn process_msg<Out>(
        &mut self,
        msg: AppMsg<EmeraldContext>,
        reply: Receiver<Out>,
    ) -> Result<Out> {
        process_consensus_message(
            msg,
            &mut self.components.state,
            &mut self.components.channels,
            &self.components.engine,
            &self.components.emerald_config,
        )
        .await
        .map_err(|err| anyhow!("Failed to process consensus message: {err:?}"))?;

        reply
            .await
            .map_err(|err| anyhow!("Failed waiting for application reply: {err}"))
    }
}
