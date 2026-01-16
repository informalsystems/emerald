//! Translates ProcessSyncedValueAction from Quint to AppMsg::ProcessSyncedValue.

use anyhow::Result;
use malachitebft_app_channel::app::types::codec::Codec;
use malachitebft_app_channel::app::types::core::Round as EmeraldRound;
use malachitebft_app_channel::AppMsg;
use malachitebft_eth_types::codec::proto::ProtobufCodec;
use malachitebft_eth_types::Height as EmeraldHeight;

use super::Sut;
use crate::history::History;
use crate::state::Proposal;

impl Sut {
    /// Replays the ProcessSyncedValue Quint action (see emerald.qnt
    /// handle_process_synced_value).
    ///
    /// This method relies on history's recorded Emerald value for the given
    /// Quint proposal.
    pub async fn process_synced_value(&mut self, hist: &History, proposal: Proposal) -> Result<()> {
        let proposer = hist.get_address(&proposal.proposer)?;
        let value = hist.get_value(&proposal.id())?;
        let value_bytes = ProtobufCodec.encode(&value)?;

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        let msg = AppMsg::ProcessSyncedValue {
            height: EmeraldHeight::new(proposal.height),
            round: EmeraldRound::new(proposal.round),
            proposer,
            value_bytes,
            reply: reply_tx,
        };

        self.process_msg(msg, reply_rx).await?;
        Ok(())
    }
}
