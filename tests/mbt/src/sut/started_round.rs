use anyhow::Result;
use malachitebft_app_channel::app::types::core::Round as EmeraldRound;
use malachitebft_app_channel::AppMsg;
use malachitebft_core_consensus::Role;
use malachitebft_eth_types::Height as EmeraldHeight;

use super::Sut;
use crate::history::History;
use crate::state::{Height, Node, Round};

impl Sut {
    pub async fn started_round(
        &mut self,
        hist: &History,
        height: Height,
        round: Round,
        proposer: Node,
    ) -> Result<()> {
        let proposer = hist.get_address(&proposer)?;
        let role = if self.address == proposer {
            Role::Proposer
        } else {
            Role::Validator
        };

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        let msg = AppMsg::StartedRound {
            height: EmeraldHeight::new(height),
            round: EmeraldRound::new(round),
            reply_value: reply_tx,
            proposer,
            role,
        };

        self.process_msg(msg, reply_rx).await?;
        Ok(())
    }
}
