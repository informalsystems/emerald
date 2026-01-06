use malachitebft_app_channel::app::types::codec::Codec;
use malachitebft_app_channel::app::types::core::Round as EmeraldRound;
use malachitebft_app_channel::AppMsg;
use malachitebft_eth_types::codec::proto::ProtobufCodec;
use malachitebft_eth_types::Height as EmeraldHeight;

use crate::driver::{process_app_message, EmeraldDriver};
use crate::state::{Node, Proposal};

impl EmeraldDriver {
    pub fn handle_process_synced_value(&mut self, node: Node, proposal: Proposal) {
        let app = self.nodes.get_mut(&node).expect("Node should exist");
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        let proposer = self
            .addresses
            .get_by_left(&proposal.proposer)
            .expect("Unknown proposer");

        let value = self
            .values
            .get(&proposal.id())
            .expect("Unknown proposal value");

        let value_bytes = ProtobufCodec.encode(value).expect("Failed to encode value");

        let msg = AppMsg::ProcessSyncedValue {
            height: EmeraldHeight::new(proposal.height),
            round: EmeraldRound::new(proposal.round),
            proposer: *proposer,
            value_bytes,
            reply: reply_tx,
        };

        self.runtime.block_on(async {
            process_app_message(app, msg).await;
            reply_rx
                .await
                .expect("Failed to process ProcessSyncedValue");
        });
    }
}
