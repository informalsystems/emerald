//! Sync handler functions for processing synced payloads

use alloy_rpc_types_engine::{ExecutionPayloadV3, PayloadStatusEnum};
use bytes::Bytes;
use color_eyre::eyre::{self, eyre};
use malachitebft_app_channel::app::types::codec::Codec;
use malachitebft_app_channel::app::types::core::{Round, Validity};
use malachitebft_app_channel::app::types::sync::RawDecidedValue;
use malachitebft_eth_engine::engine::Engine;
use malachitebft_eth_types::codec::proto::ProtobufCodec;
use malachitebft_eth_types::{BlockHash, EmeraldContext, Height, RetryConfig, Value};
use ssz::{Decode, Encode};
use tracing::{debug, error, info, warn};

use crate::state::{reconstruct_execution_payload, ValidatedPayloadCache};
use crate::store::Store;

/// Generic function to validate execution payload with retry mechanism for SYNCING status.
/// Returns the validity of the payload or an error if timeout is exceeded.
/// Uses cache to avoid duplicate validation
pub async fn validate_payload(
    cache: &mut ValidatedPayloadCache,
    engine: &Engine,
    execution_payload: &ExecutionPayloadV3,
    versioned_hashes: &[BlockHash],
    retry_config: &RetryConfig,
    height: Height,
    round: Round,
) -> eyre::Result<Validity> {
    let block_hash = execution_payload.payload_inner.payload_inner.block_hash;

    // Check if we've already called newPayload for this block
    if let Some(cached_validity) = cache.get(&block_hash) {
        debug!(
            %height, %round, %block_hash, validity = ?cached_validity,
            "Skipping duplicate newPayload call, returning cached result"
        );
        return Ok(cached_validity);
    }

    let payload_status = engine
        .notify_new_block_with_retry(
            execution_payload.clone(),
            versioned_hashes.to_vec(),
            retry_config,
        )
        .await
        .map_err(|e| {
            eyre!(
                "Execution client stuck in SYNCING for {:?} at height {}: {}",
                retry_config.max_elapsed_time,
                height,
                e
            )
        })?;

    match payload_status.status {
        PayloadStatusEnum::Valid => Ok(Validity::Valid),
        PayloadStatusEnum::Accepted => {
            warn!(%height, %round, "⚠️  Synced block ACCEPTED: {:?}, this shouldn't happen", payload_status.status);
            Ok(Validity::Invalid)
        }
        PayloadStatusEnum::Invalid { validation_error } => {
            error!(%height, %round, validation_error = ?validation_error, "🔴 Synced block INVALID");
            Ok(Validity::Invalid)
        }
        PayloadStatusEnum::Syncing => {
            unreachable!("SYNCING status should be handled by notify_new_block_with_retry")
        }
    }
}

/// Retrieves a decided value for sync at the given height.
/// If the value is pruned from storage, reconstructs it from the block header and execution layer.
pub async fn get_decided_value_for_sync(
    store: &Store,
    engine: &Engine,
    height: Height,
    earliest_unpruned_height: Height,
) -> eyre::Result<Option<RawDecidedValue<EmeraldContext>>> {
    if height >= earliest_unpruned_height {
        // Height is in our decided values table - get it directly
        info!(%height, earliest_unpruned_height = %earliest_unpruned_height, "Getting decided value from local storage");
        let decided_value = store.get_decided_value(height).await?.ok_or_else(|| {
            eyre!("Decided value not found at height {height}, data integrity error")
        })?;

        Ok(Some(RawDecidedValue {
            certificate: decided_value.certificate,
            value_bytes: ProtobufCodec.encode(&decided_value.value)?,
        }))
    } else {
        // Height has been pruned from decided values - try to reconstruct from header + EL
        info!(%height, earliest_unpruned_height = %earliest_unpruned_height, "Height pruned from storage, reconstructing from block header + EL");

        // Get certificate and block header, if not pruned
        let (certificate, header_bytes) = match store.get_certificate_and_header(height).await {
            Ok(Some((cert, header))) => (cert, header),
            Ok(None) => {
                error!(%height, "Certificate or block header not found for pruned height");
                return Ok(None);
            }
            Err(e) => {
                error!(%height, error = %e, "Failed to get certificate and header");
                return Ok(None);
            }
        };

        // Deserialize header
        let header = ExecutionPayloadV3::from_ssz_bytes(&header_bytes).map_err(|e| {
            eyre!(
                "Failed to deserialize block header at height {}: {:?}",
                height,
                e
            )
        })?;

        let block_number = header.payload_inner.payload_inner.block_number;

        // Request payload body from EL
        let bodies = engine.get_payload_bodies_by_range(block_number, 1).await?;

        // Handle response according to spec
        if bodies.is_empty() {
            // Empty array means requested range is beyond latest known block
            error!(%height, block_number, "EL returned empty array - block beyond latest known");
            return Ok(None);
        }

        let body = match bodies.first() {
            Some(Some(body)) => body,
            Some(None) => {
                // Body is null - block unavailable (pruned or not downloaded by EL)
                error!(%height, block_number, "EL returned null - block pruned or unavailable");
                return Ok(None);
            }
            None => {
                error!(%height, block_number, "EL returned unexpected empty response");
                return Ok(None);
            }
        };

        // Successfully got the body - reconstruct full payload
        info!(%height, block_number, "Successfully retrieved payload body from EL");

        let full_payload = reconstruct_execution_payload(header, body.clone());
        let payload_bytes = Bytes::from(full_payload.as_ssz_bytes());

        // Create Value from payload bytes
        let value = Value::new(payload_bytes);

        Ok(Some(RawDecidedValue {
            certificate,
            value_bytes: ProtobufCodec.encode(&value)?,
        }))
    }
}
