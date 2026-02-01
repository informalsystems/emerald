//! EVM Application for simplex consensus.
//!
//! Implements the commonware consensus Application trait using
//! emerald's Engine API client for EVM execution.
//!
//! Handles the same behaviors as the original emerald node:
//! - Validated payload caching to avoid redundant EL calls
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
use commonware_consensus::marshal::ingress::mailbox::{AncestorStream, Identifier, Mailbox};
use commonware_consensus::marshal::Update;
use commonware_consensus::simplex::types::{Activity, Context};
use commonware_consensus::types::Height;
use commonware_consensus::{Heightable, Reporter};
use commonware_cryptography::sha256::Digest;
use commonware_cryptography::{Digestible, Hasher, Sha256};
use commonware_p2p::authenticated::discovery::Oracle;
use commonware_p2p::Manager;
use commonware_runtime::{Clock, Metrics, Spawner};
use commonware_utils::vec::NonEmptyVec;
use commonware_utils::{Acknowledgement, SystemTimeExt};
use futures::StreamExt;
use rand::Rng;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::block::{Block, ExecutionData};
use crate::consensus::{PublicKey, Scheme, EPOCH};
use crate::execution_engine::{EngineClient, Fork};

/// Message that is hashed to create the parent digest for the genesis block.
/// Since the genesis block has no actual parent, this deterministic value is used.
const GENESIS_PARENT_MESSAGE: &[u8] = b"emerald-simplex genesis";

/// Seconds in the future allowed for block timestamps.
const SYNCHRONY_BOUND: u64 = 2;
const BACKFILL_POLL_INTERVAL: Duration = Duration::from_millis(200);
const BACKFILL_MAX_WAIT: Duration = Duration::from_secs(10);

/// Maximum cache size for validated payloads.
/// Aligned with emerald's cache size for consistency.
const VALIDATED_PAYLOAD_CACHE_SIZE: usize = 10;

/// Cache for tracking recently validated execution payloads.
/// Stores both the block hash and its validity result.
pub struct ValidatedPayloadCache {
    cache: AdaptiveCache<B256, PayloadStatusEnum>,
}

impl ValidatedPayloadCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: AdaptiveCache::new(max_size)
                .expect("Failed to create AdaptiveCache: invalid cache size"),
        }
    }

    /// Check if a block hash has been validated and return its cached validity.
    pub fn get(&mut self, block_hash: &B256) -> Option<PayloadStatusEnum> {
        self.cache.get(block_hash).cloned()
    }

    /// Insert a block hash and its validity result into the cache.
    pub fn insert(&mut self, block_hash: B256, status: PayloadStatusEnum) {
        self.cache.put(block_hash, status);
    }
}

/// State tracked for EVM execution.
///
/// Note: We only track heights, not hashes. Hash tracking is unnecessary because:
/// - Simplex provides canonical ancestry via AncestorStream
/// - EL forkchoice_updated is called with block.execution_hash() directly
/// - Heights are sufficient for early-exit optimizations in verify()
pub struct EvmState {
    /// Last finalized block, used for height/timestamp tracking.
    pub finalized_block: Block,
    /// Cache for validated payloads.
    pub validated_cache: ValidatedPayloadCache,
}

impl EvmState {
    pub fn new(finalized_block: Block) -> Self {
        Self {
            finalized_block,
            validated_cache: ValidatedPayloadCache::new(VALIDATED_PAYLOAD_CACHE_SIZE),
        }
    }
}

/// EVM Application that uses emerald's Engine API for block building and validation.
#[derive(Clone)]
pub struct Application {
    genesis_block: Block,
    engine: Arc<EngineClient>,
    state: Arc<RwLock<EvmState>>,
    min_block_time: Duration,
    /// Fee recipient address for block building (immutable after initialization).
    fee_recipient: Address,
    /// Prague fork activation timestamp (in seconds). None means Prague is not activated.
    prague_time: Option<u64>,
    /// Osaka fork activation timestamp (in seconds). None means Osaka is not activated.
    osaka_time: Option<u64>,
    /// P2P oracle for updating authorized peers on validator set changes.
    oracle: Oracle<PublicKey>,
    /// Mailbox for fetching finalized ancestry via marshal.
    marshal_mailbox: Mailbox<Scheme, Block>,
}

/// Reporter that logs consensus activity and forwards to inner reporter.
#[derive(Clone)]
pub struct ConsensusReporter<R> {
    inner: R,
}

impl<R> ConsensusReporter<R> {
    pub const fn new(inner: R) -> Self {
        Self { inner }
    }
}

impl<R> Reporter for ConsensusReporter<R>
where
    R: Reporter<Activity = Activity<Scheme, Digest>>,
{
    type Activity = Activity<Scheme, Digest>;

    async fn report(&mut self, activity: Self::Activity) {
        match &activity {
            Activity::Notarize(vote) => {
                debug!(
                    round = %vote.proposal.round,
                    parent = %vote.proposal.parent,
                    payload = %vote.proposal.payload,
                    "Consensus notarize"
                );
            }
            Activity::Notarization(cert) => {
                debug!(
                    round = %cert.proposal.round,
                    parent = %cert.proposal.parent,
                    payload = %cert.proposal.payload,
                    "Consensus notarization"
                );
            }
            Activity::Certification(cert) => {
                debug!(
                    round = %cert.proposal.round,
                    parent = %cert.proposal.parent,
                    payload = %cert.proposal.payload,
                    "Consensus certification"
                );
            }
            Activity::Nullify(vote) => {
                debug!(
                    round = %vote.round,
                    "Consensus nullify"
                );
            }
            Activity::Nullification(cert) => {
                debug!(
                    round = %cert.round,
                    "Consensus nullification"
                );
            }
            Activity::Finalize(vote) => {
                debug!(
                    round = %vote.proposal.round,
                    parent = %vote.proposal.parent,
                    payload = %vote.proposal.payload,
                    "Consensus finalize"
                );
            }
            Activity::Finalization(cert) => {
                debug!(
                    round = %cert.proposal.round,
                    parent = %cert.proposal.parent,
                    payload = %cert.proposal.payload,
                    "Consensus finalization"
                );
            }
            Activity::ConflictingNotarize(evidence) => {
                warn!(?evidence, "Consensus conflicting notarize");
            }
            Activity::ConflictingFinalize(evidence) => {
                warn!(?evidence, "Consensus conflicting finalize");
            }
            Activity::NullifyFinalize(evidence) => {
                warn!(?evidence, "Consensus nullify/finalize");
            }
        }

        self.inner.report(activity).await;
    }
}

impl Application {
    /// Create a new EVM application.
    ///
    /// # Arguments
    /// * `engine` - The emerald Engine API client
    /// * `fee_recipient` - Address to receive block rewards
    /// * `oracle` - P2P oracle for updating authorized peers on validator set changes
    pub async fn new(
        engine: EngineClient,
        fee_recipient: Address,
        min_block_time: Duration,
        oracle: Oracle<PublicKey>,
        marshal_mailbox: Mailbox<Scheme, Block>,
    ) -> Self {
        let genesis_execution_block = engine
            .get_genesis_block()
            .await
            .expect("Failed to fetch genesis block from execution layer");

        let genesis_execution_data = ExecutionData::from_rpc_header(genesis_execution_block.header);
        let genesis_block =
            Block::new(Sha256::hash(GENESIS_PARENT_MESSAGE), genesis_execution_data);

        let latest_execution_block = engine
            .get_latest_block()
            .await
            .expect("Failed to fetch latest block from execution layer");

        let latest_execution_data = ExecutionData::from_rpc_header(latest_execution_block.header);

        // not a correct parent hash in simplex context, but sufficient for initializing state
        let latest_block = Block::new(Sha256::hash(GENESIS_PARENT_MESSAGE), latest_execution_data);

        let state = EvmState::new(latest_block.clone());

        info!(
            gensis_height = %genesis_block.height(),
            gensis_exec_hash = %genesis_block.execution_hash(),
            latest_height = %latest_block.height(),
            latest_exec_hash = %latest_block.execution_hash(),
            "EVM Application initialized"
        );

        Self {
            genesis_block,
            engine: Arc::new(engine),
            state: Arc::new(RwLock::new(state)),
            fee_recipient,
            min_block_time,
            prague_time: Some(0), // Prague enabled from genesis by default
            osaka_time: None,     // Osaka disabled by default
            oracle,
            marshal_mailbox,
        }
    }

    /// Get the Engine API client.
    pub fn engine(&self) -> &EngineClient {
        &self.engine
    }

    /// Determine the current fork based on timestamp (in seconds).
    /// This follows the same pattern as emerald's get_fork() method.
    fn fork_for_timestamp(&self, timestamp_secs: u64) -> Fork {
        if self.osaka_time.is_some_and(|time| time <= timestamp_secs) {
            return Fork::Osaka;
        }
        if self.prague_time.is_some_and(|time| time <= timestamp_secs) {
            return Fork::Prague;
        }
        Fork::Unsupported
    }

    /// Replay blocks from ancestry to the EVM execution client.
    /// This is called when we detect that the EVM is behind the consensus ancestry.
    /// Takes the blocks that need to be checked (already collected from ancestry).
    /// Returns the highest height successfully replayed, or None if no replay was needed.
    async fn backfill_targets(&mut self) -> NonEmptyVec<PublicKey> {
        let peers = self
            .oracle
            .peer_set(EPOCH.get())
            .await
            .expect("No peer set available for backfill");
        let peer_vec: Vec<PublicKey> = peers.iter().cloned().collect();
        NonEmptyVec::try_from(peer_vec)
            .expect("Peer set was empty after conversion; cannot request backfill")
    }

    async fn fetch_finalized_block_with_hint(
        &self,
        height: Height,
        targets: &NonEmptyVec<PublicKey>,
    ) -> Block {
        let mut marshal = self.marshal_mailbox.clone();
        if let Some(block) = marshal.get_block(height).await {
            info!(
                height = %height,
                "Finalized block found locally without backfill"
            );
            return block;
        }

        marshal.hint_finalized(height, targets.clone()).await;

        let poll = async {
            loop {
                if let Some(block) = marshal.get_block(height).await {
                    info!(
                        height = %height,
                        "Finalized block fetched via backfill"
                    );
                    return block;
                }
                tokio::time::sleep(BACKFILL_POLL_INTERVAL).await;
            }
        };

        match tokio::time::timeout(BACKFILL_MAX_WAIT, poll).await {
            Ok(block) => block,
            Err(_) => panic!(
                "Timed out waiting {BACKFILL_MAX_WAIT:?} for finalized block at height {height}"
            ),
        }
    }

    async fn collect_missing_finalized_blocks(
        &mut self,
        evm_latest: Height,
        target_height: Height,
    ) -> Vec<Block> {
        if evm_latest >= target_height {
            return Vec::new();
        }

        let latest_info = self.marshal_mailbox.get_info(Identifier::Latest).await;
        let global_finalized_height = latest_info.map(|(h, _)| h);

        info!(
            evm_height = %evm_latest,
            global_finalized_height = ?global_finalized_height,
            "Fetching missing finalized blocks"
        );

        let targets = self.backfill_targets().await;

        let mut blocks = Vec::new();
        let mut height = target_height;
        while height > evm_latest {
            let block = self.fetch_finalized_block_with_hint(height, &targets).await;
            blocks.push(block);

            let Some(prev) = height.previous() else {
                break;
            };
            height = prev;
        }
        blocks.reverse();
        blocks
    }

    /// Update the EVM state after finalization.
    pub async fn on_finalized(&mut self, block: &Block) {
        let finalized_height = block.height();
        let previously_finalized_height = {
            let state = self.state.read().await;
            state.finalized_block.height()
        };

        if finalized_height <= previously_finalized_height {
            debug!(
                height = %finalized_height,
                finalized_height = %previously_finalized_height,
                "Block skipped: already processed"
            );
        } else {
            let payload = block.payload().cloned().unwrap_or_else(|| {
                panic!("Finalized block missing execution payload at height {finalized_height}")
            });

            if Self::payload_has_blobs(&payload) {
                panic!("Finalized payload includes blobs at height {finalized_height}");
            }

            // Import the payload
            let parent_hash = block.parent_execution_hash();
            let status = self.new_payload_v4(payload, vec![], parent_hash).await;

            if !status.status.is_valid() {
                panic!(
                    "Finalized payload invalid at height {}: {:?}",
                    finalized_height, status.status
                );
            }

            // Always update forkchoice to the new finalized block
            let response = self
                .forkchoice_updated_v3(block.execution_hash(), None)
                .await;
            if !response.payload_status.status.is_valid() {
                panic!(
                    "Forkchoice update returned non-valid status at height {}: {:?}",
                    finalized_height, response.payload_status.status
                );
            }
        }

        {
            let mut state = self.state.write().await;

            state.finalized_block = block.clone();
        }

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
    }

    async fn get_payload(
        &self,
        parent_exec_hash: B256,
        el_timestamp: u64,
        prev_randao: B256,
        parent_hash: B256,
        fee_recipient: &Address,
    ) -> Option<ExecutionPayloadV3> {
        let payload_attributes = PayloadAttributes {
            timestamp: el_timestamp,
            prev_randao,
            suggested_fee_recipient: *fee_recipient,
            withdrawals: Some(vec![]),
            parent_beacon_block_root: Some(parent_hash),
        };

        let ForkchoiceUpdated {
            payload_status,
            payload_id,
        } = self
            .forkchoice_updated_v3(parent_exec_hash, Some(payload_attributes))
            .await;

        if payload_status.status.is_syncing() {
            return None;
        }

        if !payload_status.status.is_valid() {
            panic!(
                "Forkchoice returned non-valid status for payload build (parent_exec_hash {}): {:?}",
                parent_exec_hash, payload_status.status
            );
        }

        let payload_id = payload_id.expect("Payload ID missing after forkchoice update");

        // Determine fork based on EL timestamp
        let fork = self.fork_for_timestamp(el_timestamp);

        // Use get_payload method with the determined fork
        let payload_envelope = match fork {
            Fork::Osaka => self.engine.get_payload_v5(payload_id).await,
            Fork::Prague => self.engine.get_payload_v4(payload_id).await,
            Fork::Unsupported => panic!(
                "Unsupported fork for timestamp {} (prague_time {:?}, osaka_time {:?})",
                el_timestamp, self.prague_time, self.osaka_time
            ),
        };

        match payload_envelope {
            Ok(payload) => Some(payload),
            Err(e) => {
                panic!("Failed to fetch execution payload via get_payload (fork {fork:?}): {e}")
            }
        }
    }

    async fn new_payload_v4(
        &self,
        execution_payload: ExecutionPayloadV3,
        versioned_hashes: Vec<B256>,
        parent_hash: B256,
    ) -> PayloadStatus {
        // Use new_payload method with Prague fork (V4)
        let result = self
            .engine
            .new_payload_v4(execution_payload, versioned_hashes, parent_hash)
            .await;

        match result {
            Ok(payload_status) => payload_status,
            Err(e) => panic!("newPayload failed: {e}"),
        }
    }

    async fn forkchoice_updated_v3(
        &self,
        head_block_hash: B256,
        payload_attributes: Option<PayloadAttributes>,
    ) -> ForkchoiceUpdated {
        let result = self
            .engine
            .fork_choice_updated_v3(head_block_hash, payload_attributes)
            .await;

        match result {
            Ok(forkchoice_updated) => forkchoice_updated,
            Err(e) => panic!("forkchoiceUpdated failed: {e}"),
        }
    }

    /// Validate execution payload with the execution engine.
    /// Uses cache to avoid duplicate validation calls.
    async fn validate_new_payload_v4(
        &self,
        payload: &ExecutionPayloadV3,
        height: Height,
        parent_hash: B256,
    ) -> PayloadStatusEnum {
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

        // Validate with execution engine
        let status = self
            .new_payload_v4(payload.clone(), versioned_hashes, parent_hash)
            .await;

        let status = status.status;

        // Cache the result
        {
            let mut state = self.state.write().await;
            state.validated_cache.insert(block_hash, status.clone());
        }

        status
    }

    /// Check if payload includes blob data (unsupported without versioned hashes).
    const fn payload_has_blobs(payload: &ExecutionPayloadV3) -> bool {
        payload.blob_gas_used != 0 || payload.excess_blob_gas != 0
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
        let genesis = self.genesis_block.clone();
        info!(
            height = %genesis.height(),
            exec_hash = %genesis.execution_hash(),
            "Returning genesis block"
        );
        genesis
    }

    async fn propose(
        &mut self,
        (runtime_context, _context): (E, Self::Context),
        mut ancestry: AncestorStream<Self::SigningScheme, Self::Block>,
    ) -> Option<Self::Block> {
        let parent = ancestry
            .next()
            .await
            .expect("Missing parent in ancestry stream for propose");
        let parent_digest = parent.digest();
        let parent_execution_hash = parent.execution_hash();

        info!(
            height = %parent.height().next(),
            parent_exec_hash = %parent_execution_hash,
            "Proposing new block on parent"
        );

        // Enforce minimum block time - wait if we're proposing too quickly.
        // Block time defines how long a transaction needs to wait to be included in a proposal block.
        // We wait to allow the mempool to fill up with transactions to include in the proposed block.
        {
            let last_timestamp_secs = {
                let state = self.state.read().await;
                state.finalized_block.timestamp
            };
            let min_next_timestamp = last_timestamp_secs + self.min_block_time.as_secs();
            let current_secs = runtime_context.current().epoch_millis() / 1000;
            if current_secs < min_next_timestamp {
                let sleep_secs = min_next_timestamp - current_secs;
                let sleep_for = Duration::from_secs(sleep_secs);
                debug!(?sleep_for, "Waiting for min_block_time before proposing");
                tokio::time::sleep(sleep_for).await;
            }
        }

        let mut current_time_secs = runtime_context.current().epoch_millis() / 1000;
        let parent_el_timestamp_secs = match parent.payload() {
            Some(payload) => payload.payload_inner.payload_inner.timestamp,
            None => {
                if parent.height() == Height::zero() {
                    0
                } else {
                    panic!(
                        "Parent block missing execution payload at height {}",
                        parent.height()
                    );
                }
            }
        };

        if current_time_secs <= parent_el_timestamp_secs {
            let target_secs = parent_el_timestamp_secs.saturating_add(1);
            let sleep_for = target_secs.saturating_sub(current_time_secs);
            if sleep_for > 0 {
                debug!(
                    ?sleep_for,
                    parent_el_timestamp_secs, "Waiting for wall clock to reach next EL timestamp"
                );
                tokio::time::sleep(Duration::from_secs(sleep_for)).await;
                current_time_secs = runtime_context.current().epoch_millis() / 1000;
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

        let current = el_timestamp;

        // Use prev_randao from the execution parent block
        let prev_randao = parent.prev_randao();

        // Convert fee recipient to emerald Address type
        let parent_hash = parent_execution_hash;

        // Build execution payload via Engine API
        let Some(payload) = self
            .get_payload(
                parent_execution_hash,
                el_timestamp,
                prev_randao,
                parent_hash,
                &self.fee_recipient,
            )
            .await
        else {
            warn!(
                parent_exec_hash = %parent_execution_hash,
                "Execution client syncing; skipping proposal"
            );
            return None;
        };

        if Self::payload_has_blobs(&payload) {
            panic!("Payload includes blobs but versioned hashes not supported");
        }

        let payload_parent_hash = payload.payload_inner.payload_inner.parent_hash;
        if payload_parent_hash != parent_execution_hash {
            panic!(
                "Payload parent hash mismatch: expected {parent_execution_hash}, got {payload_parent_hash}"
            );
        }

        // Import the payload
        let import_status = self
            .new_payload_v4(payload.clone(), vec![], parent_hash)
            .await;
        if import_status.status.is_syncing() {
            warn!(
                parent_exec_hash = %parent_execution_hash,
                "Execution client syncing during payload import; skipping proposal"
            );
            return None;
        }
        if !import_status.status.is_valid() {
            panic!(
                "newPayload returned non-valid status after build: {:?}",
                import_status.status
            );
        }

        let payload_timestamp = payload.timestamp();
        if payload_timestamp != current {
            debug!(
                payload_timestamp,
                block_timestamp = current,
                "Adjusting block timestamp to match payload"
            );
        }

        let block = Block::new_with_payload(parent_digest, payload);
        let exec_hash = block.execution_hash();

        // Cache as valid since we just built it
        {
            let mut state = self.state.write().await;
            state
                .validated_cache
                .insert(exec_hash, PayloadStatusEnum::Valid);
        }

        info!(
            height = %block.height(),
            exec_hash = %exec_hash,
            txs = block
                .payload()
                .map_or(0, |p| p.payload_inner.payload_inner.transactions.len()),
            "Proposed EVM block"
        );

        Some(block)
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
        let block = ancestry
            .next()
            .await
            .expect("Missing block in ancestry stream for verify");
        let parent = ancestry
            .next()
            .await
            .expect("Missing parent in ancestry stream for verify");

        info!(
            height = %block.height(),
            exec_hash = %block.execution_hash(),
            "Verifying block"
        );

        // Check if execution hash is already in validated cache
        {
            let mut state = self.state.write().await;
            if let Some(status) = state.validated_cache.get(&block.execution_hash()) {
                info!(
                    height = %block.height(),
                    exec_hash = %block.execution_hash(),
                    ?status,
                    "Using cached validation result"
                );
                return status.is_valid();
            }
        }

        // Basic consensus verification - timestamps must be increasing
        if block.timestamp <= parent.timestamp {
            warn!(
                height = %block.height(),
                parent_timestamp = %parent.timestamp,
                block_timestamp = %block.timestamp,
                "Block timestamp not increasing",
            );
            return false;
        }

        // Check timestamp is not too far in the future (synchrony bound)
        // Note: We don't sleep in verify() - if timestamp is beyond synchrony bound, reject.
        // The MIN_BLOCK_TIME enforcement happens in report() after finalization.
        let current = runtime_context.current().epoch_millis() / 1000;
        if block.timestamp > current + SYNCHRONY_BOUND {
            warn!(
                height = %block.height(),
                block_timestamp = %block.timestamp,
                current = %current,
                SYNCHRONY_BOUND = %SYNCHRONY_BOUND,
                "Block timestamp too far in the future",
            );
            return false;
        }

        // Verify execution payload is present
        let Some(execution_payload) = block.payload().cloned() else {
            warn!(
                height = %block.height(),
                "Block missing execution payload",
            );
            return false;
        };

        if Self::payload_has_blobs(&execution_payload) {
            warn!(
                height = %block.height(),
                "Block payload includes blobs",
            );
            return false;
        }

        // Verify payload parent hash matches parent block
        let payload_parent_hash = execution_payload.payload_inner.payload_inner.parent_hash;
        if payload_parent_hash != parent.execution_hash() {
            warn!(
                height = %block.height(),
                payload_parent_hash = %payload_parent_hash,
                parent_hash = %parent.execution_hash(),
                "Payload parent hash mismatch",
            );
            return false;
        }

        // Verify payload timestamp matches block timestamp
        let payload_timestamp = execution_payload.timestamp();
        if payload_timestamp != block.timestamp {
            warn!(
                height = %block.height(),
                payload_timestamp = %payload_timestamp,
                block_timestamp = %block.timestamp,
                "Payload timestamp mismatch",
            );
            return false;
        }

        // Validate execution payload with EL (uses cache)
        let parent_hash = block.parent_execution_hash();
        let status = self
            .validate_new_payload_v4(&execution_payload, block.height(), parent_hash)
            .await;

        if !status.is_valid() {
            warn!(
                height = %block.height(),
                status = ?status,
                "Execution payload validation failed",
            );
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

        info!(height = %block.height(), "Block verified");
        true
    }
}

impl Reporter for Application {
    type Activity = Update<Block>;

    async fn report(&mut self, activity: Self::Activity) {
        if let Update::Block(block, ack_rx) = activity {
            // report() is invoked on the marshal actor task. We must not block that task
            // or issue marshal mailbox requests inline, or we can deadlock marshal.
            // Spawn work onto a separate task and acknowledge only after it completes.
            let mut app = self.clone();
            tokio::spawn(async move {
                let height = block.height();
                let evm_latest = match app.engine.get_latest_block_number().await {
                    Ok(Some(h)) => Height::new(h),
                    Ok(None) => panic!("EVM latest block number missing in report"),
                    Err(e) => panic!("Failed to get EVM latest height in report: {e}"),
                };

                info!(
                    height = %height,
                    evm_latest = %evm_latest,
                    "Processing finalized block"
                );

                let expected_evm_height = if height == Height::zero() {
                    Height::zero()
                } else {
                    height
                        .previous()
                        .expect("Finalized height has no previous height")
                };
                let mut blocks = if evm_latest < expected_evm_height {
                    info!(
                        height = %height,
                        evm_latest = %evm_latest,
                        expected_evm_height = %expected_evm_height,
                        "EVM behind finalized height, collecting missing blocks"
                    );
                    app.collect_missing_finalized_blocks(evm_latest, expected_evm_height)
                        .await
                } else {
                    Vec::new()
                };

                if evm_latest < expected_evm_height && blocks.is_empty() {
                    panic!(
                        "Missing finalized blocks could not be fetched for replay at height {height}"
                    );
                }

                blocks.push(block);
                for block in blocks {
                    app.on_finalized(&block).await;
                    info!(
                        height = %block.height(),
                        exec_hash = %block.execution_hash(),
                        "EVM block finalized"
                    );
                }
                ack_rx.acknowledge();
            });
        }
    }
}
