use anyhow::{anyhow, Result};
use informalsystems_malachitebft_core_consensus::PeerId;
use malachitebft_app_channel::AppMsg;

use super::Sut;
use crate::history::History;
use crate::state::Proposal;

impl Sut {
    pub async fn receive_proposal(&mut self, hist: &History, proposal: Proposal) -> Result<()> {
        let peer_id = PeerId::from_multihash(Default::default())
            .map_err(|err| anyhow!("Failed to create peer id: {:?}", err))?;

        for part in hist.get_stream(&proposal.id())? {
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

            let msg = AppMsg::ReceivedProposalPart {
                from: peer_id,
                part: part.clone(),
                reply: reply_tx,
            };

            self.process_msg(msg, reply_rx).await?;
        }

        Ok(())
    }
}
