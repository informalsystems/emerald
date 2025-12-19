use informalsystems_malachitebft_core_consensus::Role;
use malachitebft_app_channel::app::types::core::Round as EmeraldRound;
use malachitebft_app_channel::AppMsg;
use malachitebft_eth_types::Height as EmeraldHeight;

use crate::driver::{process_app_message, EmeraldDriver};
use crate::state::{Height, Node, Round};

impl EmeraldDriver {
    pub fn handle_started_round(
        &mut self,
        node: Node,
        height: Height,
        round: Round,
        proposer: Node,
    ) {
        let app = self.nodes.get_mut(&node).expect("Node should exist");

        let role = if node == proposer {
            Role::Proposer
        } else {
            Role::Validator
        };

        let proposer = *self
            .addresses
            .get_by_left(&proposer)
            .expect("Unknown proposer");

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        let msg = AppMsg::StartedRound {
            height: EmeraldHeight::new(height),
            round: EmeraldRound::new(round),
            reply_value: reply_tx,
            proposer,
            role,
        };

        self.runtime.block_on(async {
            process_app_message(app, msg).await;
            reply_rx.await.expect("Failed to handle StartedRound");
        });
    }
}
