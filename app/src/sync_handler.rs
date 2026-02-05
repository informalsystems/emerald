//! Sync handler functions for retrieving decided values for sync.

use alloy_rpc_types_engine::ExecutionPayloadV3;
use bytes::Bytes;
use color_eyre::eyre::{self, eyre};
use malachitebft_app_channel::app::types::codec::Codec;
use malachitebft_app_channel::app::types::sync::RawDecidedValue;
use malachitebft_eth_engine::engine::Engine;
use malachitebft_eth_types::codec::proto::ProtobufCodec;
use malachitebft_eth_types::{EmeraldContext, Height, Value};
use ssz::{Decode, Encode};
use tracing::{error, info};

use crate::payload::reconstruct_execution_payload;
use crate::store::Store;

pub async fn get_raw_value_from_store(
    store: &Store,
    height: Height,
) -> eyre::Result<Option<RawDecidedValue<EmeraldContext>>> {
    let decided_value = store
        .get_decided_value(height)
        .await?
        .ok_or_else(|| eyre!("Decided value not found at height {height}, data integrity error"))?;

    Ok(Some(RawDecidedValue {
        certificate: decided_value.certificate,
        value_bytes: ProtobufCodec.encode(&decided_value.value)?,
    }))
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
        get_raw_value_from_store(store, height).await
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
