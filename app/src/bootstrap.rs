//! Bootstrap and initialization logic for the Emerald node.
//!
//! This module handles initializing node state from genesis or from
//! previously decided blocks after a restart.

use alloy_rpc_types_engine::{ExecutionPayloadV3, PayloadStatus, PayloadStatusEnum};
use color_eyre::eyre::{self, eyre, OptionExt};
use malachitebft_eth_cli::config::EmeraldConfig;
use malachitebft_eth_engine::engine::Engine;
use malachitebft_eth_types::{Block, BlockHash, Height};
use ssz::Decode;
use tracing::{debug, info, warn};

use crate::state::{decode_value, State};
use crate::store::Store;
use crate::validators::read_validators_from_contract;

/// Represents the range of heights that need to be replayed to the execution client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayDecision {
    /// No replay needed - execution client is aligned or ahead.
    NoReplay,
    /// Replay needed from the given start height to the end height (inclusive).
    ReplayRange { start: Height, end: Height },
}

/// Determines if block replay is needed and what range to replay.
///
/// Returns `ReplayDecision::ReplayRange` if the execution client (Reth) is behind
/// Emerald's stored height and needs to catch up.
///
/// # Arguments
/// * `reth_latest_height` - The latest height known to the execution client (None if no blocks)
/// * `emerald_stored_height` - The latest height stored in Emerald's database
pub fn determine_replay_range(
    reth_latest_height: Option<u64>,
    emerald_stored_height: Height,
) -> ReplayDecision {
    match reth_latest_height {
        Some(reth_height) if reth_height < emerald_stored_height.as_u64() => {
            // Reth is behind - we need to replay blocks
            ReplayDecision::ReplayRange {
                start: Height::new(reth_height + 1),
                end: emerald_stored_height,
            }
        }
        Some(_) => {
            // Reth is aligned or ahead
            ReplayDecision::NoReplay
        }
        None => {
            // No blocks in Reth yet - replay from genesis (height 1)
            ReplayDecision::ReplayRange {
                start: Height::new(1),
                end: emerald_stored_height,
            }
        }
    }
}

/// Error type for payload status validation during replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayPayloadError {
    /// Block was rejected as invalid by the execution client.
    Invalid { validation_error: String },
    /// Execution client returned ACCEPTED, which indicates no instant finality.
    Accepted,
    /// Execution client is still syncing.
    Syncing,
}

impl core::fmt::Display for ReplayPayloadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Invalid { validation_error } => write!(f, "invalid payload: {validation_error}"),
            Self::Accepted => write!(
                f,
                "ACCEPTED status not supported during replay (no instant finality)"
            ),
            Self::Syncing => write!(f, "execution client still syncing"),
        }
    }
}

/// Validates the payload status returned after submitting a block during replay.
///
/// During replay, only `Valid` status is acceptable. Other statuses indicate
/// problems that should abort the replay process.
pub fn validate_replay_payload_status(status: &PayloadStatus) -> Result<(), ReplayPayloadError> {
    match &status.status {
        PayloadStatusEnum::Valid => Ok(()),
        PayloadStatusEnum::Invalid { validation_error } => Err(ReplayPayloadError::Invalid {
            validation_error: validation_error.clone(),
        }),
        PayloadStatusEnum::Accepted => Err(ReplayPayloadError::Accepted),
        PayloadStatusEnum::Syncing => Err(ReplayPayloadError::Syncing),
    }
}

/// Error type for forkchoice update payload status validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkchoicePayloadError {
    /// Payload was rejected as invalid.
    Invalid { validation_error: String },
    /// Execution client returned ACCEPTED unexpectedly.
    Accepted,
    /// Execution client is syncing (should be retried).
    Syncing,
}

impl core::fmt::Display for ForkchoicePayloadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Invalid { validation_error } => write!(f, "{validation_error}"),
            Self::Accepted => write!(
                f,
                "execution engine returned ACCEPTED for payload, this should not happen"
            ),
            Self::Syncing => write!(
                f,
                "SYNCING status passed for payload, this should not happen due to retry logic"
            ),
        }
    }
}

/// Validates the payload status returned after a forkchoice update.
///
/// Only `Valid` status is acceptable for initializing from an existing block.
pub fn validate_forkchoice_payload_status(
    status: &PayloadStatus,
) -> Result<(), ForkchoicePayloadError> {
    match &status.status {
        PayloadStatusEnum::Valid => Ok(()),
        PayloadStatusEnum::Invalid { validation_error } => Err(ForkchoicePayloadError::Invalid {
            validation_error: validation_error.clone(),
        }),
        PayloadStatusEnum::Accepted => Err(ForkchoicePayloadError::Accepted),
        PayloadStatusEnum::Syncing => Err(ForkchoicePayloadError::Syncing),
    }
}

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
        validate_replay_payload_status(&payload_status)
            .map_err(|e| eyre::eyre!("Block replay failed at height {}: {}", height, e))?;
        debug!("✅ Block at height {} replayed successfully", height);

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

    // Check if Reth is behind Emerald's stored height and replay if needed
    let reth_latest_height = engine.get_latest_block_number().await?;

    match determine_replay_range(reth_latest_height, height) {
        ReplayDecision::ReplayRange { start, end } => {
            if reth_latest_height.is_some() {
                warn!(
                    "⚠️  Execution client is at height {} but Emerald has blocks up to height {}. Starting height replay.",
                    reth_latest_height.unwrap(), height
                );
            } else {
                warn!("⚠️  Execution client has no blocks, replaying from genesis");
            }
            replay_heights_to_engine(&state.store, engine, start, end, emerald_config).await?;
            info!("✅ Height replay completed successfully");
        }
        ReplayDecision::NoReplay => {
            debug!(
                "Execution client at height {} is aligned with or ahead of Emerald's stored height {}",
                reth_latest_height.unwrap_or(0), height
            );
        }
    }

    let payload_status = engine
        .send_forkchoice_updated(
            latest_block_candidate_from_store.block_hash,
            &emerald_config.retry_config,
        )
        .await?;

    validate_forkchoice_payload_status(&payload_status).map_err(|e| eyre::eyre!("{}", e))?;

    // Set consensus_height to the next height where consensus will work (the tip)
    state.consensus_height = height.increment();
    state.latest_block = Some(latest_block_candidate_from_store);
    debug!("Payload is valid");
    debug!("latest block {:?}", state.latest_block);

    // Read the validator set at the stored block - this is the validator set
    // that will be active for the NEXT height (where consensus will start)
    let block_validator_set = read_validators_from_contract(
        engine.eth.url().as_ref(),
        &state.latest_block.as_ref().unwrap().block_hash,
    )
    .await?;

    // Consensus will start at consensus_height, so we set the validator set for that height
    debug!(
        "🌈 Got validator set: {:?} for height {}",
        block_validator_set, state.consensus_height
    );
    state.set_validator_set(state.consensus_height, block_validator_set);

    Ok(())
}

#[cfg(test)]
mod tests {
    use alloy_primitives::B256;
    use alloy_rpc_types_engine::{PayloadStatus, PayloadStatusEnum};

    use super::*;

    // ==================== determine_replay_range tests ====================

    #[test]
    fn test_determine_replay_range_reth_behind() {
        // Reth is at height 5, Emerald has blocks up to height 10
        let result = determine_replay_range(Some(5), Height::new(10));
        assert_eq!(
            result,
            ReplayDecision::ReplayRange {
                start: Height::new(6),
                end: Height::new(10)
            }
        );
    }

    #[test]
    fn test_determine_replay_range_reth_aligned() {
        // Reth is at the same height as Emerald
        let result = determine_replay_range(Some(10), Height::new(10));
        assert_eq!(result, ReplayDecision::NoReplay);
    }

    #[test]
    fn test_determine_replay_range_reth_ahead() {
        // Reth is ahead of Emerald (shouldn't happen normally, but handle gracefully)
        let result = determine_replay_range(Some(15), Height::new(10));
        assert_eq!(result, ReplayDecision::NoReplay);
    }

    #[test]
    fn test_determine_replay_range_reth_no_blocks() {
        // Reth has no blocks at all
        let result = determine_replay_range(None, Height::new(10));
        assert_eq!(
            result,
            ReplayDecision::ReplayRange {
                start: Height::new(1),
                end: Height::new(10)
            }
        );
    }

    #[test]
    fn test_determine_replay_range_single_block_behind() {
        // Reth is exactly one block behind
        let result = determine_replay_range(Some(9), Height::new(10));
        assert_eq!(
            result,
            ReplayDecision::ReplayRange {
                start: Height::new(10),
                end: Height::new(10)
            }
        );
    }

    // ==================== validate_replay_payload_status tests ====================

    fn make_payload_status(status: PayloadStatusEnum) -> PayloadStatus {
        PayloadStatus {
            status,
            latest_valid_hash: Some(B256::ZERO),
        }
    }

    #[test]
    fn test_validate_replay_payload_status_valid() {
        let status = make_payload_status(PayloadStatusEnum::Valid);
        assert!(validate_replay_payload_status(&status).is_ok());
    }

    #[test]
    fn test_validate_replay_payload_status_invalid() {
        let status = make_payload_status(PayloadStatusEnum::Invalid {
            validation_error: "block gas limit exceeded".to_string(),
        });
        let result = validate_replay_payload_status(&status);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err,
            ReplayPayloadError::Invalid {
                validation_error: "block gas limit exceeded".to_string()
            }
        );
    }

    #[test]
    fn test_validate_replay_payload_status_accepted() {
        let status = make_payload_status(PayloadStatusEnum::Accepted);
        let result = validate_replay_payload_status(&status);
        assert_eq!(result, Err(ReplayPayloadError::Accepted));
    }

    #[test]
    fn test_validate_replay_payload_status_syncing() {
        let status = make_payload_status(PayloadStatusEnum::Syncing);
        let result = validate_replay_payload_status(&status);
        assert_eq!(result, Err(ReplayPayloadError::Syncing));
    }

    // ==================== validate_forkchoice_payload_status tests ====================

    #[test]
    fn test_validate_forkchoice_payload_status_valid() {
        let status = make_payload_status(PayloadStatusEnum::Valid);
        assert!(validate_forkchoice_payload_status(&status).is_ok());
    }

    #[test]
    fn test_validate_forkchoice_payload_status_invalid() {
        let status = make_payload_status(PayloadStatusEnum::Invalid {
            validation_error: "unknown ancestor".to_string(),
        });
        let result = validate_forkchoice_payload_status(&status);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err,
            ForkchoicePayloadError::Invalid {
                validation_error: "unknown ancestor".to_string()
            }
        );
    }

    #[test]
    fn test_validate_forkchoice_payload_status_accepted() {
        let status = make_payload_status(PayloadStatusEnum::Accepted);
        let result = validate_forkchoice_payload_status(&status);
        assert_eq!(result, Err(ForkchoicePayloadError::Accepted));
    }

    #[test]
    fn test_validate_forkchoice_payload_status_syncing() {
        let status = make_payload_status(PayloadStatusEnum::Syncing);
        let result = validate_forkchoice_payload_status(&status);
        assert_eq!(result, Err(ForkchoicePayloadError::Syncing));
    }

    // ==================== Error Display tests ====================

    #[test]
    fn test_replay_payload_error_display() {
        assert_eq!(
            ReplayPayloadError::Invalid {
                validation_error: "bad block".to_string()
            }
            .to_string(),
            "invalid payload: bad block"
        );
        assert!(ReplayPayloadError::Accepted
            .to_string()
            .contains("ACCEPTED"));
        assert!(ReplayPayloadError::Syncing.to_string().contains("syncing"));
    }

    #[test]
    fn test_forkchoice_payload_error_display() {
        assert_eq!(
            ForkchoicePayloadError::Invalid {
                validation_error: "bad hash".to_string()
            }
            .to_string(),
            "bad hash"
        );
        assert!(ForkchoicePayloadError::Accepted
            .to_string()
            .contains("ACCEPTED"));
        assert!(ForkchoicePayloadError::Syncing
            .to_string()
            .contains("SYNCING"));
    }
}
