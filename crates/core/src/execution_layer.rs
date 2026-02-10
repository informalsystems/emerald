use core::error::Error;
use core::fmt;
use std::vec::Vec;

pub trait Block: Send + Sync + Clone {
    type Id: Send + Sync + Clone + Eq + fmt::Debug;
    type Error: Error + Send + Sync;

    fn id(&self) -> Self::Id;
    fn parent_id(&self) -> Self::Id;
    fn height(&self) -> u64;
    fn encode(&self) -> Vec<u8>;
    fn decode(bytes: &[u8]) -> Result<Self, Self::Error>;
}

#[async_trait::async_trait]
pub trait ExecutionLayer: Send + Sync {
    type Block: Block;
    type ValidatorSet: Send + Sync + Clone;
    type Error: Error + Send + Sync;

    async fn genesis_block(&self) -> Result<Self::Block, Self::Error>;

    /// EL-specific build parameters (fee recipient, retry, fork) are held internally.
    async fn build_block(
        &self,
        parent: &Self::Block,
        timestamp: u64,
    ) -> Result<Self::Block, Self::Error>;

    async fn validate_block(&self, block: &Self::Block) -> Result<bool, Self::Error>;

    /// Returns the EL-confirmed head id so consensus can verify agreement on the tip.
    async fn finalize_block(
        &self,
        block: &Self::Block,
    ) -> Result<<Self::Block as Block>::Id, Self::Error>;

    async fn validator_set(&self, block: &Self::Block) -> Result<Self::ValidatorSet, Self::Error>;

    /// Returns `None` when the EL has no blocks yet (pre-genesis).
    async fn latest_block_height(&self) -> Result<Option<u64>, Self::Error>;

    async fn get_block_by_height(&self, height: u64) -> Result<Option<Self::Block>, Self::Error>;

    /// Returns `(is_syncing, highest_known_height)`.
    async fn is_syncing(&self) -> Result<(bool, u64), Self::Error>;

    async fn shutdown(&self) -> Result<(), Self::Error>;
}
