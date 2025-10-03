//! Sync handler functions for processing synced payloads

use bytes::Bytes;
use color_eyre::eyre::{self, eyre};
use ssz::{Decode, Encode};
use std::time::Duration;
use tracing::{error, info, warn};

use alloy_rpc_types_engine::ExecutionPayloadV3;
use malachitebft_app_channel::app::types::codec::Codec;
use malachitebft_app_channel::app::types::core::{Round, Validity};
use malachitebft_app_channel::app::types::sync::RawDecidedValue;
use malachitebft_eth_engine::engine::Engine;
use malachitebft_eth_types::codec::proto::ProtobufCodec;
use malachitebft_eth_types::{BlockHash, Height, MalakethContext, Value};

use crate::state::reconstruct_execution_payload;
use crate::store::Store;

/// Validates execution payload with retry mechanism for SYNCING status.
/// Returns the validity of the payload or an error if timeout is exceeded.
pub async fn validate_synced_payload(
    engine: &Engine,
    execution_payload: &ExecutionPayloadV3,
    versioned_hashes: &[BlockHash],
    sync_timeout: Duration,
    sync_initial_delay: Duration,
    height: Height,
    round: Round,
) -> eyre::Result<Validity> {
    let start_time = tokio::time::Instant::now();
    let timeout = sync_timeout;
    let mut retry_delay = sync_initial_delay;

    loop {
        let result = engine
            .notify_new_block(execution_payload.clone(), versioned_hashes.to_vec())
            .await;

        match result {
            Ok(payload_status) => {
                if payload_status.status.is_valid() {
                    return Ok(Validity::Valid);
                }

                if payload_status.status.is_syncing() {
                    let elapsed = start_time.elapsed();
                    if elapsed >= timeout {
                        return Err(eyre!(
                            "Execution client stuck in SYNCING for {:?} at height {}",
                            elapsed,
                            height
                        ));
                    }

                    warn!(
                        %height, %round,
                        "⚠️  Execution client SYNCING, retrying in {:?} (elapsed: {:?}/{:?})",
                        retry_delay, elapsed, timeout
                    );

                    tokio::time::sleep(retry_delay).await;
                    retry_delay = std::cmp::min(retry_delay * 2, Duration::from_secs(2));
                    continue;
                }

                // INVALID or ACCEPTED - both are treated as invalid
                // INVALID: malicious block
                // ACCEPTED: Non-canonical payload - should not happen with instant finality
                error!(%height, %round, "🔴 Synced block validation failed: {}", payload_status.status);
                return Ok(Validity::Invalid);
            }
            Err(e) => {
                error!(%height, %round, "🔴 Payload validation RPC error: {}", e);
                return Err(e);
            }
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
) -> Option<RawDecidedValue<MalakethContext>> {
    if height >= earliest_unpruned_height {
        // Height is in our decided values table - get it directly
        info!(%height, earliest_unpruned_height = %earliest_unpruned_height, "Getting decided value from local storage");
        let decided_value = store.get_decided_value(height).await.ok()??;

        Some(RawDecidedValue {
            certificate: decided_value.certificate,
            value_bytes: ProtobufCodec.encode(&decided_value.value).unwrap(),
        })
    } else {
        // Height has been pruned from decided values - try to reconstruct from header + EL
        info!(%height, earliest_unpruned_height = %earliest_unpruned_height, "Height pruned from storage, reconstructing from block header + EL");

        // Get certificate and block header atomically
        let (certificate, header_bytes) = match store.get_certificate_and_header(height).await {
            Ok(Some((cert, header))) => (cert, header),
            Ok(None) => {
                error!(%height, "Certificate or block header not found for pruned height");
                return None;
            }
            Err(e) => {
                error!(%height, error = %e, "Failed to get certificate and header");
                return None;
            }
        };

        // Deserialize header
        let header = match ExecutionPayloadV3::from_ssz_bytes(&header_bytes) {
            Ok(h) => h,
            Err(e) => {
                error!(%height, error = ?e, "Failed to deserialize block header");
                return None;
            }
        };

        let block_number = header.payload_inner.payload_inner.block_number;

        // Request payload body from EL
        let bodies = match engine.get_payload_bodies_by_range(block_number, 1).await {
            Ok(b) => b,
            Err(e) => {
                error!(%height, block_number, error = %e, "Failed to get payload body from EL");
                return None;
            }
        };

        // Handle response according to spec
        if bodies.is_empty() {
            // Empty array means requested range is beyond latest known block
            error!(%height, block_number, "EL returned empty array - block beyond latest known");
            return None;
        }

        let body = match bodies.first() {
            Some(Some(body)) => body,
            Some(None) => {
                // Body is null - block unavailable (pruned or not downloaded by EL)
                error!(%height, block_number, "EL returned null - block pruned or unavailable");
                return None;
            }
            None => {
                error!(%height, block_number, "EL returned unexpected empty response");
                return None;
            }
        };

        // Successfully got the body - reconstruct full payload
        info!(%height, block_number, "Successfully retrieved payload body from EL");

        let full_payload = reconstruct_execution_payload(header, body.clone());
        let payload_bytes = Bytes::from(full_payload.as_ssz_bytes());

        // Create Value from payload bytes
        let value = Value::new(payload_bytes);

        Some(RawDecidedValue {
            certificate,
            value_bytes: ProtobufCodec.encode(&value).unwrap(),
        })
    }
}
