use color_eyre::eyre::{self, Ok};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::debug;

use alloy_rpc_types_engine::{
    ExecutionPayloadV3, ForkchoiceUpdated, PayloadAttributes, PayloadStatus, PayloadStatusEnum,
};

use malachitebft_eth_types::{Address, BlockHash, B256};

use crate::{engine_rpc::EngineRPC, ethereum_rpc::EthereumRPC, json_structures::ExecutionBlock};
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
        if !cap.forkchoice_updated_v3 || !cap.get_payload_v3 || !cap.new_payload_v3 {
            return Err(eyre::eyre!("Engine does not support required methods"));
        }
        Ok(())
    }

    pub async fn set_latest_forkchoice_state(
        &self,
        head_block_hash: BlockHash,
    ) -> eyre::Result<BlockHash> {
        debug!("🟠 set_latest_forkchoice_state: {:?}", head_block_hash);

        let ForkchoiceUpdated {
            payload_status,
            payload_id,
        } = self.api.forkchoice_updated(head_block_hash, None).await?;

        assert!(payload_id.is_none(), "Payload ID should be None!");

        debug!("➡️ payload_status: {:?}", payload_status);

        match payload_status.status {
            PayloadStatusEnum::Valid => Ok(payload_status.latest_valid_hash.unwrap()),
            PayloadStatusEnum::Syncing if payload_status.latest_valid_hash.is_none() => {
                // From the Engine API spec:
                // 8. Client software MUST respond to this method call in the
                //    following way:
                //   * {payloadStatus: {status: SYNCING, latestValidHash: null,
                //   * validationError: null}, payloadId: null} if
                //     forkchoiceState.headBlockHash references an unknown
                //     payload or a payload that can't be validated because
                //     requisite data for the validation is missing
                Err(eyre::eyre!(
                    "headBlockHash={:?} references an unknown payload or a payload that can't be validated",
                    head_block_hash
                ))
            }
            status => Err(eyre::eyre!("Invalid payload status: {}", status)),
        }
    }

    pub async fn generate_block(
        &self,
        latest_block: &ExecutionBlock,
    ) -> eyre::Result<ExecutionPayloadV3> {
        debug!("🟠 generate_block on top of {:?}", latest_block);

        let block_hash = latest_block.block_hash;

        let payload_attributes = PayloadAttributes {
            // Unix timestamp for when the payload is expected to be executed.
            // It should be greater than that of forkchoiceState.headBlockHash.
            timestamp: latest_block.timestamp + 1,

            // prev_randao comes from the previous beacon block and influences the proposer selection mechanism.
            // prev_randao is derived from the RANDAO mix (randomness accumulator) of the parent beacon block.
            // The beacon chain generates this value using aggregated validator signatures over time.
            // The mix_hash field in the generated block will be equal to prev_randao.
            // TODO: generate value according to spec.
            prev_randao: latest_block.prev_randao,

            // TODO: provide proper address.
            suggested_fee_recipient: Address::repeat_byte(42).to_alloy_address(),

            // Cannot be None in V3.
            withdrawals: Some(vec![]),

            // Cannot be None in V3.
            parent_beacon_block_root: Some(block_hash),
        };
        let ForkchoiceUpdated {
            payload_status,
            payload_id,
        } = self
            .api
            .forkchoice_updated(block_hash, Some(payload_attributes))
            .await?;

        assert_eq!(payload_status.latest_valid_hash, Some(block_hash));

        match payload_status.status {
            PayloadStatusEnum::Valid => {
                assert!(payload_id.is_some(), "Payload ID should be Some!");
                let payload_id = payload_id.unwrap();
                // See how payload is constructed: https://github.com/ethereum/consensus-specs/blob/v1.1.5/specs/merge/validator.md#block-proposal
                Ok(self.api.get_payload(payload_id).await?)
            }
            // TODO: Handle other statuses.
            status => Err(eyre::eyre!("Invalid payload status: {}", status)),
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

    /// Returns the duration since the unix epoch.
    fn _timestamp_now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_secs()
    }
}
