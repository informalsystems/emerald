//! Translates DecidedAction from Quint to AppMsg::Decided.

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use malachitebft_app_channel::app::types::core::{
    CommitCertificate, NilOrVal, Round as EmeraldRound, SignedVote,
};
use malachitebft_app_channel::AppMsg;
use malachitebft_eth_types::{EmeraldContext, Height as EmeraldHeight, Vote};

use super::Sut;
use crate::history::History;
use crate::state::{Node, Proposal};

impl Sut {
    /// Replays the Decided Quint action (see emerald.qnt handle_decided).
    ///
    /// Note that the caller must call [mock_votes] to generate a set of mocked
    /// votes prior to calling this method.
    ///
    /// This method relies on history's recorded values and records the Emerald
    /// execution block produced for the given Quint proposal.
    pub async fn decided(
        &mut self,
        hist: &mut History,
        proposal: Proposal,
        votes: Vec<SignedVote<EmeraldContext>>,
    ) -> Result<()> {
        let height = EmeraldHeight::new(proposal.height);
        let round = EmeraldRound::new(proposal.round);
        let value_id = hist.get_value_id(&proposal)?;
        let certificate = CommitCertificate::new(height, round, value_id, votes);

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        let msg = AppMsg::Decided {
            extensions: Default::default(),
            reply: reply_tx,
            certificate,
        };

        self.process_msg(msg, reply_rx).await?;

        let state = &self.components.state;
        let block = state
            .latest_block
            .ok_or(anyhow!("Should have filled latest block"))?;

        hist.record_block(proposal, block);
        Ok(())
    }
}

/// Generates mock precommit votes from all nodes for a given proposal.
pub fn mock_votes(
    sut: &BTreeMap<Node, Sut>,
    hist: &History,
    proposal: &Proposal,
) -> Result<Vec<SignedVote<EmeraldContext>>> {
    let mut votes = Vec::new();

    let height = EmeraldHeight::new(proposal.height);
    let round = EmeraldRound::new(proposal.round);
    let value_id = hist.get_value_id(proposal)?;

    for (node, app) in sut {
        let state = &app.components.state;
        let addr = hist.get_address(node)?;
        let vote = Vote::new_precommit(height, round, NilOrVal::Val(value_id), addr);
        let sign = state.signing_provider.sign(&vote.to_sign_bytes());
        votes.push(SignedVote::new(vote, sign));
    }

    Ok(votes)
}
