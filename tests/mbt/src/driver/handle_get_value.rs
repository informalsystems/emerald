use malachitebft_app_channel::app::types::core::Round as EmeraldRound;
use malachitebft_app_channel::AppMsg;
use malachitebft_eth_types::Height as EmeraldHeight;
use tokio::time::Duration;

use crate::driver::{process_app_message, EmeraldDriver};
use crate::state::{Height, Node, Proposal, Round};

impl EmeraldDriver {
    pub fn handle_get_value(
        &mut self,
        node: Node,
        height: Height,
        round: Round,
        proposal: Proposal,
    ) {
        let app = self.nodes.get_mut(&node).expect("Unknown node");
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        let msg = AppMsg::GetValue {
            height: EmeraldHeight::new(height),
            round: EmeraldRound::new(round),
            timeout: Duration::ZERO,
            reply: reply_tx,
        };

        self.runtime.block_on(async {
            process_app_message(app, msg).await;

            let emerald_proposal = reply_rx.await.expect("Failed to handle GetValue");
            let emerald_value = emerald_proposal.value.clone();
            self.values.insert(proposal.id(), emerald_value);

            let value_id = emerald_proposal.value.id();
            let bytes = app
                .state
                .get_block_data(emerald_proposal.height, emerald_proposal.round, value_id)
                .await
                .expect("Block data must be stored after GetValue");

            let stream_msgs = app
                .state
                .stream_proposal(emerald_proposal, bytes, EmeraldRound::Nil)
                .collect();

            self.streams.insert(proposal.id(), stream_msgs);
            self.proposals.insert(proposal, value_id);
        });
    }
}
