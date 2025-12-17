use malachitebft_app_channel::app::engine::host::Next;
use malachitebft_app_channel::app::types::core::CommitCertificate;
use malachitebft_app_channel::app::types::core::NilOrVal;
use malachitebft_app_channel::app::types::core::Round as EmeraldRound;
use malachitebft_app_channel::app::types::core::SignedVote;
use malachitebft_app_channel::AppMsg;
use malachitebft_eth_types::Height as EmeraldHeight;
use malachitebft_eth_types::Vote;

use crate::driver::{process_app_message, EmeraldDriver};
use crate::state::{Node, Proposal};

impl EmeraldDriver {
    pub fn handle_decided(&mut self, node: Node, proposal: Proposal) {
        let height = EmeraldHeight::new(proposal.height);
        let round = EmeraldRound::new(proposal.round);
        let value_id = *self
            .proposals
            .get_by_left(&proposal)
            .expect("Unknown proposal");

        // Pretend everybody voted for it.
        let commits = self
            .nodes
            .iter()
            .map(|(node, app)| {
                let addr = *self.addresses.get_by_left(node).expect("Unknown node");
                let vote = Vote::new_precommit(height, round, NilOrVal::Val(value_id), addr);
                let sign = app.state.signing_provider.sign(&vote.to_sign_bytes());
                SignedVote::new(vote, sign)
            })
            .collect();

        let certificate = CommitCertificate::new(height, round, value_id, commits);
        let app = self.nodes.get_mut(&node).expect("Node should exist");
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        let msg = AppMsg::Decided {
            extensions: Default::default(),
            reply: reply_tx,
            certificate,
        };

        self.runtime.block_on(async {
            process_app_message(app, msg).await;

            let next = reply_rx.await.expect("Failed to process Decided");
            assert!(
                matches!(next, Next::Start(next_height, _) if next_height == height.increment()),
                "Should have started a new height after decision"
            );

            let block = app
                .state
                .latest_block
                .expect("Should have filled lastt block");

            self.blocks.insert(proposal.payload, block.block_hash);
        });
    }
}
