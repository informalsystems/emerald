use std::time::{Duration, SystemTime, UNIX_EPOCH};

use alloy_rpc_types_engine::{
    ExecutionPayloadV3, ForkchoiceUpdated, PayloadAttributes, PayloadStatus, PayloadStatusEnum,
};
use color_eyre::eyre;
use malachitebft_eth_types::{Address, BlockHash, RetryConfig, B256};
use tracing::{debug, error, warn};

use crate::engine_rpc::EngineRPC;
use crate::ethereum_rpc::EthereumRPC;
use crate::json_structures::ExecutionBlock;
/// RPC client for Engine API.
/// Spec: https://github.com/ethereum/execution-apis/tree/main/src/engine
pub struct Engine {
    pub api: EngineRPC,
    pub eth: EthereumRPC,
}

impl Engine {
    pub fn new(api: EngineRPC, eth: EthereumRPC) -> Self {
        Self { api, eth }
    }

    pub async fn check_capabilities(&self) -> eyre::Result<()> {
        let cap = self.api.exchange_capabilities().await?;
        if !cap.forkchoice_updated_v3
            || !cap.get_payload_v3
            || !cap.new_payload_v3
            || !cap.get_payload_bodies_by_hash_v1
            || !cap.get_payload_bodies_by_range_v1
        {
            return Err(eyre::eyre!("Engine does not support required methods"));
        }
        Ok(())
    }

    async fn forkchoice_updated_with_retry(
        &self,
        head_block_hash: BlockHash,
        payload_attributes: Option<PayloadAttributes>,
        retry_config: &RetryConfig,
    ) -> eyre::Result<ForkchoiceUpdated> {
        let fcu_future = async {
            let mut retry_delay = retry_config.initial_delay;

            loop {
                let result = self
                    .api
                    .forkchoice_updated(head_block_hash, payload_attributes.clone())
                    .await;

                match result {
                    Ok(forkchoice_updated) => {
                        if forkchoice_updated.payload_status.status.is_syncing() {
                            warn!(
                                "⚠️  Execution client SYNCING, retrying in {:?}",
                                retry_delay
                            );

                            tokio::time::sleep(retry_delay).await;
                            retry_delay = retry_config.next_delay(retry_delay);
                            continue;
                        }

                        return Ok(forkchoice_updated);
                    }
                    Err(e) => return Err(e),
                }
            }
        };

        tokio::time::timeout(retry_config.max_elapsed_time, fcu_future)
            .await
            .map_err(|_| {
                eyre::eyre!(
                    "Timeout after {:?} waiting for execution client to sync",
                    retry_config.max_elapsed_time
                )
            })?
    }

    pub async fn send_forkchoice_updated(
        &self,
        head_block_hash: BlockHash,
        retry_config: &RetryConfig,
    ) -> eyre::Result<PayloadStatus> {
        debug!("🟠 send_forkchoice_updated: {:?}", head_block_hash);

        self.forkchoice_updated_with_retry(head_block_hash, None, retry_config)
            .await
            .map(|ForkchoiceUpdated { payload_status, .. }| payload_status)
    }

    pub async fn set_latest_forkchoice_state(
        &self,
        head_block_hash: BlockHash,
        retry_config: &RetryConfig,
    ) -> eyre::Result<BlockHash> {
        debug!("🟠 set_latest_forkchoice_state: {:?}", head_block_hash);

        let ForkchoiceUpdated {
            payload_status,
            payload_id,
        } = self
            .forkchoice_updated_with_retry(head_block_hash, None, retry_config)
            .await?;

        assert!(payload_id.is_none(), "Payload ID should be None!");

        debug!("➡️ payload_status: {:?}", payload_status);

        match payload_status.status {
            PayloadStatusEnum::Valid => Ok(payload_status.latest_valid_hash.unwrap()),
            PayloadStatusEnum::Accepted => {
                warn!(block_hash = ?head_block_hash, "⚠️  Forkchoice update ACCEPTED, this shouldn't happen");
                Err(eyre::eyre!("Forkchoice update returned ACCEPTED status"))
            }
            PayloadStatusEnum::Invalid { validation_error } => {
                error!(block_hash = ?head_block_hash, validation_error = ?validation_error, "🔴 Forkchoice update INVALID");
                Err(eyre::eyre!(
                    "Forkchoice update returned INVALID: {:?}",
                    validation_error
                ))
            }
            PayloadStatusEnum::Syncing => {
                unreachable!("SYNCING status should be handled by forkchoice_updated_with_retry")
            }
        }
    }

    pub async fn generate_block(
        &self,
        latest_block: &Option<ExecutionBlock>,
        retry_config: &RetryConfig,
    ) -> eyre::Result<ExecutionPayloadV3> {
        debug!("🟠 generate_block on top of {:?}", latest_block);
        let payload_attributes: PayloadAttributes;
        let block_hash: BlockHash;
        match latest_block {
            Some(lb) => {
                block_hash = lb.block_hash;

                payload_attributes = PayloadAttributes {
                    // Unix timestamp for when the payload is expected to be executed.
                    // It should be greater than that of forkchoiceState.headBlockHash.
                    timestamp: lb.timestamp + 1,

                    // prev_randao comes from the previous beacon block and influences the proposer selection mechanism.
                    // prev_randao is derived from the RANDAO mix (randomness accumulator) of the parent beacon block.
                    // The beacon chain generates this value using aggregated validator signatures over time.
                    // The mix_hash field in the generated block will be equal to prev_randao.
                    // TODO: generate value according to spec.
                    prev_randao: lb.prev_randao,

                    // TODO: provide proper address.
                    suggested_fee_recipient: Address::repeat_byte(42).to_alloy_address(),

                    // Cannot be None in V3.
                    withdrawals: Some(vec![]),

                    // Cannot be None in V3.
                    parent_beacon_block_root: Some(block_hash),
                };
            }
            None => {
                // TODO once validated that this is never happening
                panic!("lb should never be none")
            }
        }

        let ForkchoiceUpdated {
            payload_status,
            payload_id,
        } = self
            .forkchoice_updated_with_retry(block_hash, Some(payload_attributes), retry_config)
            .await?;

        assert_eq!(payload_status.latest_valid_hash, Some(block_hash));

        match payload_status.status {
            PayloadStatusEnum::Valid => {
                assert!(payload_id.is_some(), "Payload ID should be Some!");
                let payload_id = payload_id.unwrap();
                // See how payload is constructed: https://github.com/ethereum/consensus-specs/blob/v1.1.5/specs/merge/validator.md#block-proposal
                Ok(self.api.get_payload(payload_id).await?)
            }
            PayloadStatusEnum::Accepted => {
                warn!(block_hash = ?block_hash, "⚠️  Block generation forkchoice update ACCEPTED, this shouldn't happen");
                Err(eyre::eyre!(
                    "Block generation forkchoice update returned ACCEPTED status"
                ))
            }
            PayloadStatusEnum::Invalid { validation_error } => {
                error!(block_hash = ?block_hash, validation_error = ?validation_error, "🔴 Block generation forkchoice update INVALID");
                Err(eyre::eyre!(
                    "Block generation forkchoice update returned INVALID: {:?}",
                    validation_error
                ))
            }
            PayloadStatusEnum::Syncing => {
                unreachable!("SYNCING status should be handled by forkchoice_updated_with_retry")
            }
        }
    }

    pub async fn notify_new_block(
        &self,
        execution_payload: ExecutionPayloadV3,
        versioned_hashes: Vec<B256>,
    ) -> eyre::Result<PayloadStatus> {
        let parent_block_hash = execution_payload.payload_inner.payload_inner.parent_hash;
        let execution_requests = vec![]; // TODO: Implement execution requests
        self.api
            .new_payload(
                execution_payload,
                versioned_hashes,
                parent_block_hash,
                execution_requests,
            )
            .await
    }

    /// Get execution payload bodies by their block hashes
    pub async fn get_payload_bodies_by_hash(
        &self,
        block_hashes: Vec<BlockHash>,
    ) -> eyre::Result<Vec<Option<crate::json_structures::ExecutionPayloadBodyV1>>> {
        debug!("🟠 get_payload_bodies_by_hash: {:?}", block_hashes);
        self.api.get_payload_bodies_by_hash(block_hashes).await
    }

    /// Get execution payload bodies by block number range
    pub async fn get_payload_bodies_by_range(
        &self,
        start_block: u64,
        count: u64,
    ) -> eyre::Result<Vec<Option<crate::json_structures::ExecutionPayloadBodyV1>>> {
        debug!(
            "🟠 get_payload_bodies_by_range: start={}, count={}",
            start_block, count
        );
        self.api
            .get_payload_bodies_by_range(start_block, count)
            .await
    }

    /// Notifies the execution client of a new block with retry mechanism for SYNCING status.
    /// Returns the payload status or an error if timeout is exceeded.
    pub async fn notify_new_block_with_retry(
        &self,
        execution_payload: ExecutionPayloadV3,
        versioned_hashes: Vec<BlockHash>,
        retry_config: &RetryConfig,
    ) -> eyre::Result<PayloadStatus> {
        let validation_future = async {
            let mut retry_delay = retry_config.initial_delay;

            loop {
                let result = self
                    .notify_new_block(execution_payload.clone(), versioned_hashes.clone())
                    .await;

                match result {
                    Ok(payload_status) => {
                        if payload_status.status.is_syncing() {
                            warn!(
                                "⚠️  Execution client SYNCING, retrying in {:?}",
                                retry_delay
                            );

                            tokio::time::sleep(retry_delay).await;
                            retry_delay = retry_config.next_delay(retry_delay);
                            continue;
                        }

                        return Ok(payload_status);
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        };

        tokio::time::timeout(retry_config.max_elapsed_time, validation_future)
            .await
            .map_err(|_| {
                eyre::eyre!(
                    "Timeout after {:?} waiting for execution client to sync",
                    retry_config.max_elapsed_time
                )
            })?
    }

    /// Returns the duration since the unix epoch.
    fn _timestamp_now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_secs()
    }
}
