//! Execution payload utilities for validation, caching, and manipulation.

use alloy_rpc_types_engine::{ExecutionPayloadV1, ExecutionPayloadV2, ExecutionPayloadV3};
use bytes::Bytes;
use caches::lru::AdaptiveCache;
use caches::Cache;
use color_eyre::eyre::{self, eyre};
use malachitebft_app_channel::app::types::core::Validity;
use malachitebft_eth_engine::engine::Engine;
use malachitebft_eth_engine::json_structures::ExecutionPayloadBodyV1;
use malachitebft_eth_types::{Block, BlockHash, Height, RetryConfig};
use malachitebft_app_channel::app::types::core::Round;
use ssz::Decode;
use tracing::{debug, error, warn};

/// Cache for tracking recently validated execution payloads to avoid redundant validation.
/// Stores both the block hash and its validity result (Valid or Invalid).
pub struct ValidatedPayloadCache {
    cache: AdaptiveCache<BlockHash, Validity>,
}

impl ValidatedPayloadCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: AdaptiveCache::new(max_size)
                .expect("Failed to create AdaptiveCache: invalid cache size"),
        }
    }

    /// Check if a block hash has been validated and return its cached validity
    pub fn get(&mut self, block_hash: &BlockHash) -> Option<Validity> {
        self.cache.get(block_hash).copied()
    }

    /// Insert a block hash and its validity result into the cache
    pub fn insert(&mut self, block_hash: BlockHash, validity: Validity) {
        self.cache.put(block_hash, validity);
    }
}

/// Validates an already-decoded execution payload with the execution engine.
/// Uses cache to avoid duplicate validation calls.
///
/// Returns `Ok(Validity::Valid)` if valid, `Ok(Validity::Invalid)` if invalid,
/// or `Err` for engine communication failures.
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

    let validity = if payload_status.status.is_valid() {
        Validity::Valid
    } else {
        // INVALID or ACCEPTED - both are treated as invalid
        // INVALID: malicious block
        // ACCEPTED: Non-canonical payload - should not happen with instant finality
        error!(%height, %round, "Block validation failed: {}", payload_status.status);
        Validity::Invalid
    };

    cache.insert(block_hash, validity);
    Ok(validity)
}

/// Validates execution payload bytes with the execution engine.
/// Decodes the payload, extracts versioned hashes, and validates.
///
/// Returns `Ok(Validity::Invalid)` if decoding fails or payload is invalid,
/// `Ok(Validity::Valid)` if valid, or `Err` for engine communication failures.
pub async fn validate_execution_payload(
    cache: &mut ValidatedPayloadCache,
    data: &Bytes,
    height: Height,
    round: Round,
    engine: &Engine,
    retry_config: &RetryConfig,
) -> eyre::Result<Validity> {
    // Decode execution payload
    let execution_payload = match ExecutionPayloadV3::from_ssz_bytes(data) {
        Ok(payload) => payload,
        Err(e) => {
            warn!(
                height = %height,
                round = %round,
                error = ?e,
                "Proposal has invalid ExecutionPayloadV3 encoding"
            );
            return Ok(Validity::Invalid);
        }
    };

    // Extract versioned hashes for blob transactions
    let block: Block = match execution_payload.clone().try_into_block() {
        Ok(block) => block,
        Err(e) => {
            warn!(
                height = %height,
                round = %round,
                error = ?e,
                "Failed to convert ExecutionPayloadV3 to Block"
            );
            return Ok(Validity::Invalid);
        }
    };
    let versioned_hashes: Vec<BlockHash> =
        block.body.blob_versioned_hashes_iter().copied().collect();

    // Validate with execution engine
    validate_payload(
        cache,
        engine,
        &execution_payload,
        &versioned_hashes,
        retry_config,
        height,
        round,
    )
    .await
}

/// Extracts a block header from an ExecutionPayloadV3 by removing transactions and withdrawals.
///
/// Returns an ExecutionPayloadV3 with empty transactions and withdrawals vectors,
/// containing only the block header fields.
pub fn extract_block_header(payload: &ExecutionPayloadV3) -> ExecutionPayloadV3 {
    ExecutionPayloadV3 {
        payload_inner: ExecutionPayloadV2 {
            payload_inner: ExecutionPayloadV1 {
                transactions: vec![],
                ..payload.payload_inner.payload_inner.clone()
            },
            withdrawals: vec![],
        },
        ..payload.clone()
    }
}

/// Reconstructs a complete ExecutionPayloadV3 from a block header and payload body.
///
/// Takes a header (ExecutionPayloadV3 with empty transactions/withdrawals) and combines it
/// with the transactions and withdrawals from an ExecutionPayloadBodyV1 to create a full payload.
pub fn reconstruct_execution_payload(
    header: ExecutionPayloadV3,
    body: ExecutionPayloadBodyV1,
) -> ExecutionPayloadV3 {
    ExecutionPayloadV3 {
        payload_inner: ExecutionPayloadV2 {
            payload_inner: ExecutionPayloadV1 {
                transactions: body.transactions,
                ..header.payload_inner.payload_inner
            },
            withdrawals: body.withdrawals.unwrap_or_default(),
        },
        ..header
    }
}
