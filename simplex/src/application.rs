//! EVM Application for simplex consensus.
//!
//! Implements the commonware consensus Application trait using
//! emerald's Engine API client for EVM execution.
//!
//! Handles the same behaviors as the original emerald node:
//! - Validated payload caching to avoid redundant EL calls
//! - Retry handling for syncing EL nodes
//! - Proper timestamp validation
//! - Minimum block time enforcement

use core::time::Duration;
use std::sync::Arc;

use alloy_primitives::{Address, B256};
use alloy_rpc_types_engine::{
    ExecutionPayloadV3, ForkchoiceUpdated, PayloadAttributes, PayloadStatus, PayloadStatusEnum,
};
use caches::lru::AdaptiveCache;
use caches::Cache;
use commonware_consensus::marshal::ingress::mailbox::AncestorStream;
use commonware_consensus::marshal::Update;
use commonware_consensus::simplex::types::Context;
use commonware_consensus::types::Height;
use commonware_consensus::{Heightable, Reporter};
use commonware_cryptography::sha256::Digest;
use commonware_cryptography::{Digestible, Hasher, Sha256};
use commonware_p2p::authenticated::discovery::Oracle;
use commonware_p2p::Manager;
use commonware_runtime::{Clock, Metrics, Spawner};
use commonware_utils::{Acknowledgement, SystemTimeExt};
use futures::StreamExt;
use malachitebft_eth_engine::engine::Engine as EmeraldEngine;
use malachitebft_eth_engine::engine_rpc::Fork;
use malachitebft_eth_types::RetryConfig;
use rand::Rng;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::block::{Block, ExecutionHash};
use crate::consensus::{PublicKey, Scheme};

/// Message that is hashed to create the parent digest for the genesis block.
/// Since the genesis block has no actual parent, this deterministic value is used.
const GENESIS_PARENT_MESSAGE: &[u8] = b"emerald-simplex genesis";

/// Milliseconds in the future allowed for block timestamps.
const SYNCHRONY_BOUND: u64 = 2_000;

/// Maximum cache size for validated payloads.
/// Aligned with emerald's cache size for consistency.
const VALIDATED_PAYLOAD_CACHE_SIZE: usize = 10;

/// Validity result for payload validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Validity {
    Valid,
    Invalid,
}

/// Cache for tracking recently validated execution payloads.
/// Stores both the block hash and its validity result.
pub struct ValidatedPayloadCache {
    cache: AdaptiveCache<B256, Validity>,
}

impl ValidatedPayloadCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: AdaptiveCache::new(max_size)
                .expect("Failed to create AdaptiveCache: invalid cache size"),
        }
    }

    /// Check if a block hash has been validated and return its cached validity.
    pub fn get(&mut self, block_hash: &B256) -> Option<Validity> {
        self.cache.get(block_hash).copied()
    }

    /// Insert a block hash and its validity result into the cache.
    pub fn insert(&mut self, block_hash: B256, validity: Validity) {
        self.cache.put(block_hash, validity);
    }
}

/// State tracked for EVM execution.
///
/// Note: We only track heights, not hashes. Hash tracking is unnecessary because:
/// - Simplex provides canonical ancestry via AncestorStream
/// - EL forkchoice_updated is called with block.execution_hash() directly
/// - Heights are sufficient for early-exit optimizations in verify()
pub struct EvmState {
    /// Height of the last finalized block.
    pub finalized_height: Height,
    /// Height of the last notarized block.
    pub notarized_height: Height,
    /// Timestamp (in seconds) of last finalized EVM block, used to enforce minimum block time.
    pub last_block_timestamp: u64,
    /// Cache for validated payloads.
    pub validated_cache: ValidatedPayloadCache,
}

impl Default for EvmState {
    fn default() -> Self {
        Self {
            finalized_height: Height::zero(),
            notarized_height: Height::zero(),
            last_block_timestamp: 0,
            validated_cache: ValidatedPayloadCache::new(VALIDATED_PAYLOAD_CACHE_SIZE),
        }
    }
}

/// EVM Application that uses emerald's Engine API for block building and validation.
#[derive(Clone)]
pub struct Application {
    genesis: Arc<Block>,
    engine: Arc<EmeraldEngine>,
    state: Arc<RwLock<EvmState>>,
    retry_config: RetryConfig,
    min_block_time: Duration,
    /// Fee recipient address for block building (immutable after initialization).
    fee_recipient: Address,
    /// Prague fork activation timestamp (in seconds). None means Prague is not activated.
    prague_time: Option<u64>,
    /// Osaka fork activation timestamp (in seconds). None means Osaka is not activated.
    osaka_time: Option<u64>,
    /// P2P oracle for updating authorized peers on validator set changes.
    oracle: Oracle<PublicKey>,
}

impl Application {
    /// Create a new EVM application.
    ///
    /// # Arguments
    /// * `engine` - The emerald Engine API client
    /// * `fee_recipient` - Address to receive block rewards
    /// * `genesis_execution_hash` - The EL genesis block hash
    /// * `oracle` - P2P oracle for updating authorized peers on validator set changes
    pub fn new(
        engine: EmeraldEngine,
        fee_recipient: Address,
        genesis_execution_hash: ExecutionHash,
        min_block_time: Duration,
        oracle: Oracle<PublicKey>,
    ) -> Self {
        let genesis = Block::new(
            Sha256::hash(GENESIS_PARENT_MESSAGE),
            Height::zero(),
            0,
            genesis_execution_hash,
        );

        Self {
            genesis: Arc::new(genesis),
            engine: Arc::new(engine),
            state: Arc::new(RwLock::new(EvmState::default())),
            fee_recipient,
            retry_config: RetryConfig::default(),
            min_block_time,
            prague_time: Some(0), // Prague enabled from genesis by default
            osaka_time: None,     // Osaka disabled by default
            oracle,
        }
    }

    /// Create a new EVM application with custom retry configuration.
    pub fn with_retry_config(mut self, retry_config: RetryConfig) -> Self {
        self.retry_config = retry_config;
        self
    }

    /// Get the Engine API client.
    pub fn engine(&self) -> &EmeraldEngine {
        &self.engine
    }

    /// Determine the current fork based on timestamp (in seconds).
    /// This follows the same pattern as emerald's get_fork() method.
    fn get_fork(&self, timestamp_secs: u64) -> Fork {
        if self.osaka_time.is_some_and(|time| time <= timestamp_secs) {
            return Fork::Osaka;
        }
        if self.prague_time.is_some_and(|time| time <= timestamp_secs) {
            return Fork::Prague;
        }
        Fork::Unsupported
    }

    /// Update the EVM state after finalization.
    pub async fn on_finalized(&mut self, block: &Block) {
        let mut state = self.state.write().await;

        let finalized_height = block.height();

        // Warn if notarized_height wasn't already at the new finalized_height
        // This could indicate blocks were finalized without being marked safe first
        if state.notarized_height != finalized_height {
            warn!(
                notarized_height = %state.notarized_height,
                old_finalized_height = %state.finalized_height,
                new_finalized_height = %finalized_height,
                "notarized_height wasn't at finalized_height before finalization"
            );
            state.notarized_height = finalized_height;
        }

        state.finalized_height = finalized_height;

        // TODO: Dynamic validator set updates
        // To implement validator set changes:
        // 1. Read validators from on-chain contract at block.execution_hash()
        // 2. Compare with current validator set
        // 3. If changed, update self.oracle.update(next_epoch, new_validators).await
        // 4. Also update the consensus scheme provider for the new epoch

        {
            let peer_set = self.oracle.peer_set(0).await;
            info!(?peer_set, "Current authorized peer set from oracle");
        }

        info!(
            height = %finalized_height,
            exec_hash = %block.execution_hash(),
            "EVM block finalized"
        );
    }

    /// Check if payload includes blob data (unsupported without versioned hashes).
    fn payload_has_blobs(payload: &ExecutionPayloadV3) -> bool {
        payload.blob_gas_used != 0 || payload.excess_blob_gas != 0
    }

    /// Generate a deterministic prev_randao value from the consensus context.
    fn generate_prev_randao(parent_digest: &Digest, height: Height) -> B256 {
        let mut hasher = Sha256::new();
        hasher.update(parent_digest);
        hasher.update(&height.get().to_be_bytes());
        hasher.update(b"prevrandao");
        B256::from_slice(&hasher.finalize())
    }

    /// Derive a deterministic parent beacon block root from the parent consensus digest.
    fn parent_beacon_block_root(parent_digest: &Digest) -> B256 {
        B256::from_slice(&parent_digest.0)
    }

    async fn forkchoice_updated_with_retry(
        &self,
        head_block_hash: B256,
        payload_attributes: Option<PayloadAttributes>,
    ) -> Result<ForkchoiceUpdated, String> {
        let fcu_future = async {
            let mut retry_delay = self.retry_config.initial_delay;

            loop {
                let result = self
                    .engine
                    .api
                    .forkchoice_updated(head_block_hash, payload_attributes.clone())
                    .await;

                match result {
                    Ok(forkchoice_updated) => {
                        if forkchoice_updated.payload_status.status.is_syncing() {
                            warn!("Execution client SYNCING, retrying in {:?}", retry_delay);

                            tokio::time::sleep(retry_delay).await;
                            retry_delay = self.retry_config.next_delay(retry_delay);
                            continue;
                        }

                        return Ok(forkchoice_updated);
                    }
                    Err(e) => return Err(format!("{e}")),
                }
            }
        };

        tokio::time::timeout(self.retry_config.max_elapsed_time, fcu_future)
            .await
            .map_err(|_| {
                format!(
                    "Timeout after {:?} waiting for execution client to sync",
                    self.retry_config.max_elapsed_time
                )
            })?
    }

    async fn notify_new_block_with_retry(
        &self,
        execution_payload: ExecutionPayloadV3,
        versioned_hashes: Vec<B256>,
        parent_beacon_block_root: B256,
    ) -> Result<PayloadStatus, String> {
        let validation_future = async {
            let mut retry_delay = self.retry_config.initial_delay;

            loop {
                // Use new_payload method with Prague fork (V4)
                let result: Result<PayloadStatus, _> = self
                    .engine
                    .api
                    .new_payload(
                        execution_payload.clone(),
                        versioned_hashes.clone(),
                        parent_beacon_block_root,
                        vec![], // execution_requests (empty for now)
                    )
                    .await;

                match result {
                    Ok(payload_status) => {
                        if payload_status.status.is_syncing() {
                            warn!("Execution client SYNCING, retrying in {:?}", retry_delay);

                            tokio::time::sleep(retry_delay).await;
                            retry_delay = self.retry_config.next_delay(retry_delay);
                            continue;
                        }

                        return Ok(payload_status);
                    }
                    Err(e) => return Err(format!("{e}")),
                }
            }
        };

        tokio::time::timeout(self.retry_config.max_elapsed_time, validation_future)
            .await
            .map_err(|_| {
                format!(
                    "Timeout after {:?} waiting for execution client to sync",
                    self.retry_config.max_elapsed_time
                )
            })?
    }

    async fn build_execution_payload(
        &self,
        parent_exec_hash: B256,
        el_timestamp: u64,
        prev_randao: B256,
        parent_beacon_block_root: B256,
        fee_recipient: &malachitebft_eth_types::Address,
    ) -> Option<ExecutionPayloadV3> {
        let payload_attributes = PayloadAttributes {
            timestamp: el_timestamp,
            prev_randao,
            suggested_fee_recipient: fee_recipient.to_alloy_address(),
            withdrawals: Some(vec![]),
            parent_beacon_block_root: Some(parent_beacon_block_root),
        };

        let ForkchoiceUpdated {
            payload_status,
            payload_id,
        } = match self
            .forkchoice_updated_with_retry(parent_exec_hash, Some(payload_attributes))
            .await
        {
            Ok(response) => response,
            Err(e) => {
                error!(error = %e, "Failed to update forkchoice for payload build");
                return None;
            }
        };

        if !payload_status.status.is_valid() {
            error!(status = ?payload_status.status, "Forkchoice returned non-valid status");
            return None;
        }

        let Some(payload_id) = payload_id else {
            error!("Payload ID missing after forkchoice update");
            return None;
        };

        // Determine fork based on EL timestamp
        let fork = self.get_fork(el_timestamp);

        // Use get_payload method with the determined fork
        let payload_envelope = match self.engine.api.get_payload(payload_id, fork).await {
            Ok(payload) => Some(payload),
            Err(e) => {
                error!(error = %e, fork = ?fork, "Failed to fetch execution payload via get_payload");
                None
            }
        };

        payload_envelope
    }

    /// Validate execution payload with the execution engine.
    /// Uses cache to avoid duplicate validation calls.
    async fn validate_execution_payload(
        &self,
        payload: &ExecutionPayloadV3,
        height: Height,
        parent_beacon_block_root: B256,
    ) -> Validity {
        let block_hash = payload.payload_inner.payload_inner.block_hash;

        // Check cache first
        {
            let mut state = self.state.write().await;
            if let Some(cached_validity) = state.validated_cache.get(&block_hash) {
                debug!(
                    %height, %block_hash, validity = ?cached_validity,
                    "Returning cached payload validation result"
                );
                return cached_validity;
            }
        }

        // Extract versioned hashes for blob transactions (empty for non-blob txs)
        let versioned_hashes: Vec<B256> = vec![];

        // Validate with execution engine using retry mechanism
        let result = self
            .notify_new_block_with_retry(
                payload.clone(),
                versioned_hashes,
                parent_beacon_block_root,
            )
            .await;

        let validity = match result {
            Ok(status) if status.status.is_valid() => Validity::Valid,
            Ok(status) => {
                warn!(
                    %height, %block_hash, status = ?status.status,
                    "Payload validation returned non-valid status"
                );
                Validity::Invalid
            }
            Err(e) => {
                // Timeout or other error during validation - treat as invalid
                // This handles the case where EL is stuck syncing
                warn!(
                    %height, %block_hash, error = %e,
                    "Payload validation failed (EL may be syncing)"
                );
                Validity::Invalid
            }
        };

        // Cache the result
        {
            let mut state = self.state.write().await;
            state.validated_cache.insert(block_hash, validity);
        }

        validity
    }

    /// Update safe/head forkchoice for a notarized block.
    pub async fn on_notarized(&self, block: &Block) {
        let (notarized_height, finalized_height) = {
            let state = self.state.read().await;
            (state.notarized_height, state.finalized_height)
        };

        // Skip if block is already processed
        if block.height() <= notarized_height || block.height() <= finalized_height {
            debug!(
                height = %block.height(),
                notarized_height = %notarized_height,
                finalized_height = %finalized_height,
                "Block skipped: already processed"
            );
            return;
        }

        let Some(payload) = block.payload().cloned() else {
            warn!(height = %block.height(), "Notarized block missing execution payload");
            return;
        };

        if Self::payload_has_blobs(&payload) {
            warn!(height = %block.height(), "Notarized payload includes blobs");
            return;
        }

        // Import the payload with retry for syncing nodes
        let parent_beacon_block_root = Self::parent_beacon_block_root(&block.parent);
        let import_result = self
            .notify_new_block_with_retry(payload, vec![], parent_beacon_block_root)
            .await;

        match import_result {
            Ok(status) if matches!(status.status, PayloadStatusEnum::Valid) => {}
            Ok(status) if matches!(status.status, PayloadStatusEnum::Syncing) => {
                warn!(height = %block.height(), "EL is syncing during notarized payload import");
                return;
            }
            Ok(status) => {
                warn!(height = %block.height(), ?status, "Notarized payload invalid");
                return;
            }
            Err(e) => {
                warn!(height = %block.height(), ?e, "Failed to import notarized payload");
                return;
            }
        }

        // Update forkchoice
        let forkchoice_result = self
            .engine
            .set_latest_forkchoice_state(block.execution_hash(), &self.retry_config)
            .await;

        match forkchoice_result {
            Ok(_) => {}
            Err(e) => {
                warn!(height = %block.height(), ?e, "Failed to update safe forkchoice");
                return;
            }
        }

        let mut state = self.state.write().await;
        if block.height() > state.notarized_height {
            state.notarized_height = block.height();
            info!(height = %block.height(), "Marked block as notarized");
        }
    }
}

impl<E> commonware_consensus::Application<E> for Application
where
    E: Rng + Spawner + Metrics + Clock + Send + Sync + 'static,
{
    type SigningScheme = Scheme;
    type Context = Context<Digest, PublicKey>;
    type Block = Block;

    async fn genesis(&mut self) -> Self::Block {
        self.genesis.as_ref().clone()
    }

    async fn propose(
        &mut self,
        (runtime_context, _context): (E, Self::Context),
        mut ancestry: AncestorStream<Self::SigningScheme, Self::Block>,
    ) -> Option<Self::Block> {
        let parent = ancestry.next().await?;
        let parent_height = parent.height;
        let parent_digest = parent.digest();
        let parent_exec_hash = parent.execution_hash();

        // Enforce minimum block time - wait if we're proposing too quickly.
        // Block time defines how long a transaction needs to wait to be included in a proposal block.
        // We wait to allow the mempool to fill up with transactions to include in the proposed block.
        {
            let state = self.state.read().await;
            let last_timestamp_secs = state.last_block_timestamp;
            let min_next_timestamp = last_timestamp_secs + self.min_block_time.as_secs();
            let current_secs = runtime_context.current().epoch_millis() / 1000;
            if current_secs < min_next_timestamp {
                let sleep_secs = min_next_timestamp - current_secs;
                let sleep_for = Duration::from_secs(sleep_secs);
                debug!(?sleep_for, "Waiting for min_block_time before proposing");
                tokio::time::sleep(sleep_for).await;
            }
        }

        let mut current_time_millis = runtime_context.current().epoch_millis();
        let mut current_time_secs = current_time_millis / 1000;
        let parent_el_timestamp_secs = parent
            .payload()
            .map(|p| p.payload_inner.payload_inner.timestamp)
            .unwrap_or(0);

        if current_time_secs <= parent_el_timestamp_secs {
            let target_millis = parent_el_timestamp_secs.saturating_add(1) * 1000;
            let sleep_for = target_millis.saturating_sub(current_time_millis);
            if sleep_for > 0 {
                debug!(
                    ?sleep_for,
                    parent_el_timestamp_secs, "Waiting for wall clock to reach next EL timestamp"
                );
                tokio::time::sleep(Duration::from_millis(sleep_for)).await;
                current_time_millis = runtime_context.current().epoch_millis();
                current_time_secs = current_time_millis / 1000;
            }
        }

        let min_timestamp = parent_el_timestamp_secs.saturating_add(1);
        let el_timestamp = current_time_secs.max(min_timestamp);

        if el_timestamp > current_time_secs {
            debug!(
                current_time_secs,
                parent_el_timestamp_secs,
                el_timestamp,
                "Using future timestamp to satisfy EL constraint"
            );
        }

        let current = el_timestamp * 1000; // consensus timestamp in ms

        // Generate deterministic consensus-derived values for EL payload attributes
        let prev_randao = Self::generate_prev_randao(&parent_digest, parent_height.next());
        let parent_beacon_block_root = Self::parent_beacon_block_root(&parent_digest);

        // Convert fee recipient to emerald Address type
        let fee_recipient = malachitebft_eth_types::Address::from(alloy_primitives::Address::from(
            self.fee_recipient.0 .0,
        ));

        // Build execution payload via Engine API with retry
        let payload_result = self
            .build_execution_payload(
                parent_exec_hash,
                el_timestamp,
                prev_randao,
                parent_beacon_block_root,
                &fee_recipient,
            )
            .await;

        match payload_result {
            Some(payload) => {
                if Self::payload_has_blobs(&payload) {
                    error!("Payload includes blobs but versioned hashes not supported");
                    return None;
                }

                let payload_parent_hash = payload.payload_inner.payload_inner.parent_hash;
                if payload_parent_hash != parent_exec_hash {
                    error!(
                        ?payload_parent_hash,
                        ?parent_exec_hash,
                        "Payload parent hash mismatch"
                    );
                    return None;
                }

                // Import the payload with retry
                let import_result = self
                    .notify_new_block_with_retry(payload.clone(), vec![], parent_beacon_block_root)
                    .await;

                match import_result {
                    Ok(status) if matches!(status.status, PayloadStatusEnum::Valid) => {}
                    Ok(status) => {
                        error!(?status, "newPayload returned non-valid status after build");
                        return None;
                    }
                    Err(e) => {
                        error!(?e, "Failed to import payload via newPayload");
                        return None;
                    }
                }

                let payload_timestamp_ms = payload.timestamp().saturating_mul(1000);
                if payload_timestamp_ms != current {
                    debug!(
                        payload_timestamp = payload.timestamp(),
                        block_timestamp = current,
                        "Adjusting block timestamp to match payload"
                    );
                }

                let new_height = parent_height.next();
                let block = Block::new_with_payload(
                    parent_digest,
                    new_height,
                    payload_timestamp_ms,
                    payload,
                );
                let exec_hash = block.execution_hash();

                // Cache as valid since we just built it
                {
                    let mut state = self.state.write().await;
                    state.validated_cache.insert(exec_hash, Validity::Valid);
                }

                info!(
                    height = %new_height,
                    exec_hash = %exec_hash,
                    txs = block
                        .payload()
                        .map(|p| p.payload_inner.payload_inner.transactions.len())
                        .unwrap_or(0),
                    "Proposed EVM block"
                );

                Some(block)
            }
            None => {
                error!("Failed to build execution payload");
                None
            }
        }
    }
}

impl<E> commonware_consensus::VerifyingApplication<E> for Application
where
    E: Rng + Spawner + Metrics + Clock + Send + Sync + 'static,
{
    #[allow(unused_mut)]
    async fn verify(
        &mut self,
        (mut runtime_context, _): (E, Self::Context),
        mut ancestry: AncestorStream<Self::SigningScheme, Self::Block>,
    ) -> bool {
        let Some(block) = ancestry.next().await else {
            return false;
        };
        let Some(parent) = ancestry.next().await else {
            return false;
        };

        // Check if execution hash is already in validated cache
        {
            let mut state = self.state.write().await;
            if let Some(validity) = state.validated_cache.get(&block.execution_hash()) {
                debug!(
                    height = %block.height(),
                    exec_hash = %block.execution_hash(),
                    ?validity,
                    "Using cached validation result"
                );
                return validity == Validity::Valid;
            }
        }

        // Basic consensus verification - timestamps must be increasing
        if block.timestamp <= parent.timestamp {
            warn!(height = %block.height(), "Block timestamp not increasing");
            return false;
        }

        // Check timestamp is not too far in the future (synchrony bound)
        // Note: We don't sleep in verify() - if timestamp is beyond synchrony bound, reject.
        // The MIN_BLOCK_TIME enforcement happens in report() after finalization.
        let current = runtime_context.current().epoch_millis();
        if block.timestamp > current + SYNCHRONY_BOUND {
            warn!(
                height = %block.height(),
                timestamp = block.timestamp,
                current,
                "Block timestamp too far in future"
            );
            return false;
        }

        // Verify execution payload is present
        let Some(execution_payload) = block.payload().cloned() else {
            warn!(height = %block.height(), "Block missing execution payload");
            return false;
        };

        if Self::payload_has_blobs(&execution_payload) {
            warn!(height = %block.height(), "Block payload includes blobs");
            return false;
        }

        // Verify payload parent hash matches parent block
        let payload_parent_hash = execution_payload.payload_inner.payload_inner.parent_hash;
        if payload_parent_hash != parent.execution_hash() {
            warn!(
                height = %block.height(),
                payload_parent = %payload_parent_hash,
                expected = %parent.execution_hash(),
                "Payload parent hash mismatch"
            );
            return false;
        }

        // Verify payload timestamp matches block timestamp
        let payload_timestamp = execution_payload.timestamp();
        if payload_timestamp.saturating_mul(1000) != block.timestamp {
            warn!(
                height = %block.height(),
                payload_timestamp,
                block_timestamp = block.timestamp,
                "Payload timestamp mismatch"
            );
            return false;
        }

        // Verify execution hash consistency
        let payload_exec_hash = execution_payload.payload_inner.payload_inner.block_hash;
        if payload_exec_hash != block.execution_hash() {
            warn!(
                height = %block.height(),
                block_exec_hash = %block.execution_hash(),
                payload_exec_hash = %payload_exec_hash,
                "Execution hash mismatch"
            );
            return false;
        }

        // Validate execution payload with EL (uses cache)
        let parent_beacon_block_root = Self::parent_beacon_block_root(&block.parent);
        let validity = self
            .validate_execution_payload(
                &execution_payload,
                block.height(),
                parent_beacon_block_root,
            )
            .await;

        if validity == Validity::Invalid {
            warn!(height = %block.height(), "Execution payload validation failed");
            return false;
        }

        // Byzantine behavior for testing: randomly reject valid blocks with probability p.
        // With n=4 validators (quorum=3) and p=0.38:
        //   - P(0 reject) = 0.62^4 = 15%  -> progress
        //   - P(1 reject) = 4*0.38*0.62^3 = 36% -> progress (still have 3 votes)
        //   - P(2+ reject) = 49% -> nullification (< 3 votes)
        // This gives ~51% progress rate and ~49% nullification rate.
        #[cfg(feature = "byzantine-test")]
        {
            const BYZANTINE_REJECT_PROBABILITY: f64 = 0.38;
            if runtime_context.gen_bool(BYZANTINE_REJECT_PROBABILITY) {
                warn!(height = %block.height(), "Byzantine test: randomly rejecting valid block");
                return false;
            }
        }

        debug!(height = %block.height(), "Block verified");
        true
    }
}

impl Reporter for Application {
    type Activity = Update<Block>;

    async fn report(&mut self, activity: Self::Activity) {
        if let Update::Block(block, ack_rx) = activity {
            let height = block.height();

            // Update finalized state first
            self.on_finalized(&block).await;

            // Import the execution payload if present
            // This is necessary because our node may not have this block
            // (it was built by another validator)
            if let Some(payload) = block.payload() {
                if !Self::payload_has_blobs(payload) {
                    let parent_beacon_block_root = Self::parent_beacon_block_root(&block.parent);
                    let import_result = self
                        .notify_new_block_with_retry(
                            payload.clone(),
                            vec![],
                            parent_beacon_block_root,
                        )
                        .await;

                    if let Err(e) = &import_result {
                        warn!(error = %e, %height, "Failed to import finalized payload");
                    }
                }
            }

            // Update forkchoice to finalize this block in the EL
            let result = self
                .engine
                .set_latest_forkchoice_state(block.execution_hash(), &self.retry_config)
                .await;

            if let Err(e) = &result {
                warn!(?e, %height, "Failed to update EL forkchoice for finalized block");
            }

            // Update last block timestamp for min_block_time enforcement in propose()
            // Use the EVM block's timestamp
            if let Some(payload) = block.payload() {
                let block_timestamp = payload.payload_inner.payload_inner.timestamp;
                let mut state = self.state.write().await;
                state.last_block_timestamp = block_timestamp;
            }

            ack_rx.acknowledge();
        }
    }
}
