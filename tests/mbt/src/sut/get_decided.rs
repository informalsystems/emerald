use anyhow::{bail, Result};
use malachitebft_app_channel::AppMsg;
use malachitebft_eth_types::Height as EmeraldHeight;

use super::Sut;
use crate::history::History;
use crate::state::{Height, Proposal};

impl Sut {
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

        if proposal != emerald_proposal {
            bail!(
                "Spec and emerald decided values diverge: spec={:?} emerald={:?}",
                proposal,
                emerald_proposal
            );
        }

        Ok(())
    }
}
