use bytes::Bytes;
use malachitebft_app_channel::app::types::codec::Codec;
use malachitebft_app_channel::app::types::core::Round as EmeraldRound;
use malachitebft_app_channel::AppMsg;
use malachitebft_eth_types::codec::proto::ProtobufCodec;
use malachitebft_eth_types::Height as EmeraldHeight;

use crate::driver::{process_app_message, EmeraldDriver};
use crate::state::{Node, Proposal};

impl EmeraldDriver {
    pub fn handle_process_synced_value(&mut self, node: Node, proposal: Proposal) {
        // Get the value for this proposal
        let value = self
            .values
            .get(&proposal.id())
            .expect("Unknown proposal value");

        // Get the proposer's address
        let proposer_address = self
            .addresses
            .get_by_left(&proposal.proposer)
            .expect("Unknown proposer");

        // Get the block data from the proposer's node (not the syncing node)
        // We need to do this before getting a mutable borrow on the target node
        let proposer_app = self
            .nodes
            .get(&proposal.proposer)
            .expect("Proposer node should exist");

        let block_bytes = self.runtime.block_on(async {
            // Get the block data (extensions) from the proposer's store
            proposer_app
                .state
                .get_block_data(
                    EmeraldHeight::new(proposal.height),
                    EmeraldRound::new(proposal.round),
                    value.id(),
                )
                .await
                .expect("Block data must exist on proposer for synced value")
        });

        // Now get the mutable borrow for the target node
        let app = self.nodes.get_mut(&node).expect("Node should exist");

        self.runtime.block_on(async {
            // Encode the value (combining consensus data and block data)
            let value_with_extensions = {
                let mut v = value.clone();
                v.extensions = block_bytes;
                v
            };

            let value_bytes: Bytes = ProtobufCodec
                .encode(&value_with_extensions)
                .expect("Failed to encode value");

            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

            let msg = AppMsg::ProcessSyncedValue {
                height: EmeraldHeight::new(proposal.height),
                round: EmeraldRound::new(proposal.round),
                proposer: *proposer_address,
                value_bytes,
                reply: reply_tx,
            };

            process_app_message(app, msg).await;

            reply_rx
                .await
                .expect("Failed to process ProcessSyncedValue");
        });
    }
}
