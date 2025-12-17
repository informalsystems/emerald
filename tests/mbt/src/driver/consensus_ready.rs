use malachitebft_app_channel::AppMsg;

use crate::driver::{process_app_message, EmeraldDriver};
use crate::state::Node;

impl EmeraldDriver {
    pub fn handle_consensus_ready(&mut self, node: Node) {
        let app = self.nodes.get_mut(&node).expect("Unknown node");
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let msg = AppMsg::ConsensusReady { reply: reply_tx };

        self.runtime.block_on(async {
            process_app_message(app, msg).await;
            reply_rx.await.expect("Failed to handle ConsensusReady");
        });
    }
}
