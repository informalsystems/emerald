//! Translates GetDecidedValueAction from Quint to AppMsg::GetDecidedValue.

use anyhow::{ensure, Result};
use malachitebft_app_channel::AppMsg;
use malachitebft_eth_types::Height as EmeraldHeight;

use super::Sut;
use crate::history::History;
use crate::state::{Height, Proposal};

impl Sut {
    /// Replays the GetDecidedValue Quint action (see emerald.qnt
    /// handle_get_decided_value).
    ///
    /// This method relies on history's recorded proposals to assert that the
    /// Emerald proposal returned is equal to the expected Quint proposal.
    pub async fn get_decided(
        &mut self,
        hist: &History,
        height: Height,
        proposal: Option<Proposal>,
    ) -> Result<()> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        let msg = AppMsg::GetDecidedValue {
            height: EmeraldHeight::new(height),
            reply: reply_tx,
        };

        let emerald_proposal = self
            .process_msg(msg, reply_rx)
            .await?
            .map(|decided| {
                let height = decided.certificate.height;
                let round = decided.certificate.round;
                let value_id = decided.certificate.value_id;
                hist.get_proposal(height, round, value_id)
            })
            .transpose()?;

        ensure!(
            proposal == emerald_proposal,
            "Spec and emerald decided values diverge: spec={proposal:?} emerald={emerald_proposal:?}"
        );

        Ok(())
    }
}
