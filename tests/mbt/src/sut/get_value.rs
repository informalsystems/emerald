//! Translates GetValueAction from Quint to AppMsg::GetValue.

use anyhow::{anyhow, Result};
use malachitebft_app_channel::app::types::core::Round as EmeraldRound;
use malachitebft_app_channel::AppMsg;
use malachitebft_eth_types::Height as EmeraldHeight;
use tokio::time::Duration;

use super::Sut;
use crate::history::History;
use crate::state::{Height, Proposal, Round};

impl Sut {
    /// Replays the GetValue Quint action (see emerald.qnt handle_get_value).
    ///
    /// This method records the Emerald value and message parts for the given
    /// Quint proposal.
    pub async fn get_value(
        &mut self,
        hist: &mut History,
        height: Height,
        round: Round,
        proposal: Proposal,
    ) -> Result<()> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        let msg = AppMsg::GetValue {
            height: EmeraldHeight::new(height),
            round: EmeraldRound::new(round),
            timeout: Duration::ZERO,
            reply: reply_tx,
        };

        let emerald_proposal = self.process_msg(msg, reply_rx).await?;
        let height = emerald_proposal.height;
        let round = emerald_proposal.round;
        let value = emerald_proposal.value.clone();
        let value_id = emerald_proposal.value.id();

        let state = &mut self.components.state;
        let block_data = state
            .get_block_data(height, round, value_id)
            .await
            .ok_or(anyhow!("No block data for value id: {value_id}"))?;

        let proposal_parts = state
            .stream_proposal(emerald_proposal, block_data, EmeraldRound::Nil)
            .collect();

        hist.record_proposal(proposal, value, proposal_parts);
        Ok(())
    }
}
