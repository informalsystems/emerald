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
        expected_proposal: Proposal,
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

            let proposal = reply_rx.await.expect("Failed to handle GetValue");
            let value_id = proposal.value.id();

            let bytes = app
                .state
                .get_block_data(proposal.height, proposal.round, value_id)
                .await
                .expect("Block data must be stored after GetValue");

            let stream_msgs = app
                .state
                .stream_proposal(proposal, bytes, EmeraldRound::Nil)
                .collect();

            self.streams.insert(expected_proposal.id(), stream_msgs);
            self.proposals.insert(expected_proposal, value_id);
        });
    }
}
