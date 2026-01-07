use malachitebft_app_channel::AppMsg;
use malachitebft_eth_types::Height as EmeraldHeight;

use crate::driver::{process_app_message, EmeraldDriver};
use crate::state::{Height, Node, Proposal};

impl EmeraldDriver {
    pub fn handle_get_decided(&mut self, node: Node, height: Height, proposal: Option<Proposal>) {
        let app = self.nodes.get_mut(&node).expect("Unknown node");
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        let msg = AppMsg::GetDecidedValue {
            height: EmeraldHeight::new(height),
            reply: reply_tx,
        };

        self.runtime.block_on(async {
            process_app_message(app, msg).await;

            let emerald_proposal = reply_rx
                .await
                .expect("Failed to handle GetDecidedValue")
                .and_then(|raw_decided| {
                    let value_id = raw_decided.certificate.value_id;
                    self.proposals.get_by_right(&value_id)
                });

            assert_eq!(proposal.as_ref(), emerald_proposal);
        });
    }
}
