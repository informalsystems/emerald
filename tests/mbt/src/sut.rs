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
use emerald::node::StateComponents;
use malachitebft_app_channel::AppMsg;
use malachitebft_eth_types::{Address, EmeraldContext};
use tokio::sync::oneshot::Receiver;

pub struct Sut {
    pub address: Address,
    pub components: StateComponents,
}

impl Sut {
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
        .map_err(|err| anyhow!("Failed to process consensus message: {:?}", err))?;

        reply
            .await
            .map_err(|err| anyhow!("Failed waiting for application reply: {}", err))
    }
}
