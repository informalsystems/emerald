//! Bootstrap and initialization logic for the Emerald node.
//!
//! This module handles initializing node state from genesis or from
//! previously decided blocks after a restart.

use alloy_rpc_types_engine::{ExecutionPayloadV3, PayloadStatusEnum};
use color_eyre::eyre::{self, eyre, OptionExt};
use malachitebft_eth_cli::config::EmeraldConfig;
use malachitebft_eth_engine::engine::Engine;
use malachitebft_eth_types::{Block, BlockHash, Height};
use ssz::Decode;
use tracing::{debug, info, warn};

use crate::state::{decode_value, State};
use crate::store::Store;
use crate::validators::read_validators_from_contract;

pub async fn initialize_state_from_genesis(state: &mut State, engine: &Engine) -> eyre::Result<()> {
    // Get the genesis block from the execution engine
    let genesis_block = engine
        .eth
        .get_block_by_number("earliest")
        .await?
        .ok_or_eyre("Genesis block does not exist")?;
    debug!("👉 genesis_block: {:?}", genesis_block);
    state.latest_block = Some(genesis_block);
    let genesis_validator_set =
        read_validators_from_contract(engine.eth.url().as_ref(), &genesis_block.block_hash).await?;
    debug!("🌈 Got genesis validator set: {:?}", genesis_validator_set);
    // Set consensus_height to the next height where consensus will work (the tip)
    state.consensus_height = Height::new(genesis_block.block_number).increment();
    state.set_validator_set(state.consensus_height, genesis_validator_set);
    Ok(())
}

/// Replay blocks from Emerald's store to the execution client (Reth).
/// This is needed when Reth is behind Emerald's stored height after a crash.
async fn replay_heights_to_engine(
    store: &Store,
    engine: &Engine,
    start_height: Height,
    end_height: Height,
    emerald_config: &EmeraldConfig,
) -> eyre::Result<()> {
    info!(
        "🔄 Replaying heights {} to {} to execution client",
        start_height, end_height
    );

    for height in start_height.as_u64()..=end_height.as_u64() {
        let height = Height::new(height);

        // Sending the whole block to the execution engine.
        let value_bytes = store
            .get_raw_decided_value(height)
            .await?
            .ok_or_else(|| {
                eyre!("Decided value not found at height {height}, data integrity error")
            })?
            .value_bytes;

        let value = decode_value(value_bytes);
        let block_bytes = value.extensions.clone();
        // Deserialize the execution payload
        let execution_payload = ExecutionPayloadV3::from_ssz_bytes(&block_bytes).map_err(|e| {
            eyre!(
                "Failed to deserialize execution payload at height {}: {:?}",
                height,
                e
            )
        })?;

        debug!(
            "🔄 Replaying block at height {} with hash {:?}",
            height, execution_payload.payload_inner.payload_inner.block_hash
        );

        // Extract versioned hashes from blob transactions
        let block: Block = execution_payload.clone().try_into_block().map_err(|e| {
            eyre!(
                "Failed to convert execution payload to block at height {}: {}",
                height,
                e
            )
        })?;
        let versioned_hashes: Vec<BlockHash> =
            block.body.blob_versioned_hashes_iter().copied().collect();

        // Submit the block to Reth
        let payload_status = engine
            .notify_new_block_with_retry(
                execution_payload.clone(),
                versioned_hashes,
                &emerald_config.retry_config,
            )
            .await?;

        // Verify the block was accepted
        match payload_status.status {
            PayloadStatusEnum::Valid => {
                debug!("✅ Block at height {} replayed successfully", height);
            }
            PayloadStatusEnum::Invalid { validation_error } => {
                return Err(eyre::eyre!(
                    "Block replay failed at height {}: {}",
                    height,
                    validation_error
                ));
            }
            PayloadStatusEnum::Accepted => {
                // ACCEPTED is no instant finality and there is a possibility of a fork.
                return Err(eyre::eyre!(
                    "Block replay failed at height {}: execution client returned ACCEPTED status, which is not supported during replay",
                    height
                ));
            }
            PayloadStatusEnum::Syncing => {
                return Err(eyre::eyre!(
                    "Block replay failed at height {}: execution client still syncing",
                    height
                ));
            }
        }

        // Update forkchoice to this block
        engine
            .set_latest_forkchoice_state(
                execution_payload.payload_inner.payload_inner.block_hash,
                &emerald_config.retry_config,
            )
            .await?;

        debug!("🎯 Forkchoice updated to height {}", height);
    }

    info!("✅ Successfully replayed all heights to execution client");
    Ok(())
}

/// Initialize state from a previously decided block stored locally by catching the
/// execution client up to that height, updating forkchoice, and loading the validator
/// set for the next consensus height.
pub async fn initialize_state_from_existing_block(
    state: &mut State,
    engine: &Engine,
    height: Height,
    emerald_config: &EmeraldConfig,
) -> eyre::Result<()> {
    // If there was somethign stored in the store for height, we should be able to retrieve
    // block data as well.

    let latest_block_candidate_from_store = state
        .get_latest_block_candidate(height)
        .await
        .ok_or_eyre("we have not atomically stored the last block, database corrupted")?;

    // Check if Reth is behind Emerald's stored height
    let reth_latest_height = engine.get_latest_block_number().await?;

    match reth_latest_height {
        Some(reth_height) if reth_height < height.as_u64() => {
            // Reth is behind - we need to replay blocks
            warn!(
                "⚠️  Execution client is at height {} but Emerald has blocks up to height {}. Starting height replay.",
                reth_height, height
            );

            // Replay from Reth's next height to Emerald's stored height
            let replay_start = Height::new(reth_height + 1);
            replay_heights_to_engine(&state.store, engine, replay_start, height, emerald_config)
                .await?;

            info!("✅ Height replay completed successfully");
        }
        Some(reth_height) => {
            debug!(
                "Execution client at height {} is aligned with or ahead of Emerald's stored height {}",
                reth_height, height
            );
        }
        None => {
            // No blocks in Reth yet (genesis case) - this shouldn't happen here
            // but handle it gracefully
            warn!("⚠️  Execution client has no blocks, replaying from genesis");
            replay_heights_to_engine(&state.store, engine, Height::new(1), height, emerald_config)
                .await?;
        }
    }

    let payload_status = engine
        .send_forkchoice_updated(
            latest_block_candidate_from_store.block_hash,
            &emerald_config.retry_config,
        )
        .await?;
    match payload_status.status {
        PayloadStatusEnum::Valid => {
            // Set consensus_height to the next height where consensus will work (the tip)
            state.consensus_height = height.increment();
            state.latest_block = Some(latest_block_candidate_from_store);
            // From the Engine API spec:
            // 8. Client software MUST respond to this method call in the
            //    following way:
            //   * {payloadStatus: {status: SYNCING, latestValidHash: null,
            //   * validationError: null}, payloadId: null} if
            //     forkchoiceState.headBlockHash references an unknown
            //     payload or a payload that can't be validated because
            //     requisite data for the validation is missing
            debug!("Payload is valid");
            debug!("latest block {:?}", state.latest_block);

            // Read the validator set at the stored block - this is the validator set
            // that will be active for the NEXT height (where consensus will start)
            let block_validator_set = read_validators_from_contract(
                engine.eth.url().as_ref(),
                &latest_block_candidate_from_store.block_hash,
            )
            .await?;

            // Consensus will start at consensus_height, so we set the validator set for that height
            debug!("🌈 Got validator set: {:?} for height {}", block_validator_set, state.consensus_height);
            state.set_validator_set(state.consensus_height, block_validator_set);

            Ok(())
        }
        PayloadStatusEnum::Invalid { validation_error } => Err(eyre::eyre!(validation_error)),

        PayloadStatusEnum::Accepted => Err(eyre::eyre!(
            "execution engine returned ACCEPTED for payload, this should not happen"
        )),
        PayloadStatusEnum::Syncing => Err(eyre::eyre!(
            "SYNCING status passed for payload, this should not happen due to retry logic in send_forkchoice_updated function"
        )),
    }
}
