use anyhow::Result;
use malachitebft_app_channel::AppMsg;

use super::Sut;

impl Sut {
    pub async fn consensus_ready(&mut self) -> Result<()> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let msg = AppMsg::ConsensusReady { reply: reply_tx };
        self.process_msg(msg, reply_rx).await?;
        Ok(())
    }
}
