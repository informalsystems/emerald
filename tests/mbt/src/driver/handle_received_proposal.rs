use informalsystems_malachitebft_core_consensus::PeerId;
use malachitebft_app_channel::AppMsg;

use crate::driver::{process_app_message, EmeraldDriver};
use crate::state::{Node, Proposal};

impl EmeraldDriver {
    pub fn handle_received_proposal(&mut self, node: Node, proposal: Proposal) {
        let app = self.nodes.get_mut(&node).expect("Node should exist");

        self.runtime.block_on(async {
            let peer_id = PeerId::from_multihash(Default::default())
                .expect("Failed to build default peer id");

            for part in self
                .streams
                .get(&proposal.id())
                .expect("Unknown proposal stream")
            {
                let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

                let msg = AppMsg::ReceivedProposalPart {
                    from: peer_id,
                    part: part.clone(),
                    reply: reply_tx,
                };

                process_app_message(app, msg).await;

                reply_rx
                    .await
                    .expect("Failed to process ReceivedProposalPart");
            }
        });
    }
}
