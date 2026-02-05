//! Internal state of the application. This is a simplified abstract to keep it simple.
//! A regular application would have mempool implemented, a proper database and input methods like RPC.

use std::fmt;

use alloy_genesis::ChainConfig;
use alloy_rpc_types_engine::ExecutionPayloadV3;
use bytes::Bytes;
use color_eyre::eyre;
use malachitebft_app_channel::app::streaming::{StreamContent, StreamId, StreamMessage};
use malachitebft_app_channel::app::types::codec::Codec;
use malachitebft_app_channel::app::types::core::{CommitCertificate, Context, Round, Validity};
use malachitebft_app_channel::app::types::{LocallyProposedValue, PeerId, ProposedValue};
use malachitebft_eth_engine::engine::Engine;
use malachitebft_eth_engine::engine_rpc::Fork;
use malachitebft_eth_engine::json_structures::ExecutionBlock;
use malachitebft_eth_types::codec::proto::ProtobufCodec;
use malachitebft_eth_types::secp256k1::K256Provider;
use malachitebft_eth_types::{
    Address, BlockTimestamp, EmeraldContext, Genesis, Height, ProposalData, ProposalFin,
    ProposalInit, ProposalPart, RetryConfig, ValidatorSet, Value, ValueId,
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use sha3::Digest;
use ssz::{Decode, Encode};
use tokio::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::metrics::Metrics;
use crate::payload::{extract_block_header, validate_execution_payload, ValidatedPayloadCache};
use crate::store::Store;
use crate::streaming::{PartStreamsMap, ProposalParts};

pub struct StateMetrics {
    pub txs_count: u64,
    pub chain_bytes: u64,
    pub elapsed_seconds: u64,
    pub metrics: Metrics,
}

/// Size of randomly generated blocks in bytes
#[allow(dead_code)]
const BLOCK_SIZE: usize = 10 * 1024 * 1024; // 10 MiB

/// Size of chunks in which the data is split for streaming
const CHUNK_SIZE: usize = 128 * 1024; // 128 KiB

/// Represents the internal state of the application node
/// Contains information about current height, round, proposals and blocks
pub struct State {
    #[allow(dead_code)]
    ctx: EmeraldContext,
    pub signing_provider: K256Provider,
    address: Address,
    pub store: Store,
    stream_nonce: u32,
    streams_map: PartStreamsMap,
    #[allow(dead_code)]
    rng: StdRng,

    /// The height where consensus is working (the tip of the blockchain).
    /// After deciding on height H, this is set to H+1.
    /// This represents the next height where consensus will propose, vote, and commit.
    pub consensus_height: Height,
    /// The round of consensus at consensus_height.
    /// Reset to 0 when advancing to a new height.
    pub consensus_round: Round,

    pub latest_block: Option<ExecutionBlock>,

    validator_set: Option<(Height, ValidatorSet)>,

    // Cache for tracking recently validated payloads to avoid duplicate validation
    validated_payload_cache: ValidatedPayloadCache,

    // For stats
    pub txs_count: u64,
    pub chain_bytes: u64,
    pub start_time: Instant,
    pub metrics: Metrics,

    /// Maximum number of certificates to keep in store.
    /// Note that when a certificate is pruend we will not be
    /// able to validated blocks on this node.
    pub num_certificates_to_retain: u64,

    /// The certificates are pruned every prune_at_block_interval heights.
    /// This is done to avoid DB access overhead.
    pub prune_at_block_interval: u64,

    /// Number of blocks to retain temporary.
    /// The block data is stored in the execution engine and therefore we do
    /// not need it in Emerald.
    /// WARN: If the exection engine is not persisting every block
    /// this parameter has to be >= than the number of blocks
    /// not persisted, otherwise crash replay will not work.
    pub num_temp_blocks_retained: u64,

    /// Minimum time of a block. If set to something > 0
    /// and a block is produces in `t` where `t` < `min_block_time`
    /// we will sleep for `min_block_time - t`.
    pub min_block_time: Duration,

    /// Time it took to execute last block.
    /// Used to decide on whether we should sleep in case min_block_time
    /// is set.
    pub last_block_time: Instant,

    /// Tracks when the previous block was committed (for per-block TPS calculation)
    pub previous_block_commit_time: Instant,

    /// Needed to extract chain configuration contained in the ethereum genesis file.
    /// Currently used to read information on the fork supported by the chain.
    pub eth_chain_config: ChainConfig,
}

/// Represents errors that can occur during the verification of a proposal's signature.
#[derive(Debug)]
pub enum SignatureVerificationError {
    /// Indicates that the `Init` part of the proposal is unexpectedly missing.
    MissingInitPart,
    /// Indicates that the `Fin` part of the proposal is unexpectedly missing.
    MissingFinPart,
    /// Indicates that the proposer was not found in the validator set.
    ProposerNotFound,
    /// Indicates that the signature in the `Fin` part is invalid.
    InvalidSignature,
    /// Validator set not found for the given height
    ValidatorSetNotFound { _height: Height },
}

/// Represents errors that can occur during proposal validation.
#[derive(Debug)]
pub enum ProposalValidationError {
    /// Proposer doesn't match the expected proposer for the given round
    WrongProposer { actual: Address, expected: Address },
    /// Signature verification errors
    Signature(SignatureVerificationError),
    /// Validator set not found for the given height
    ValidatorSetNotFound { height: Height },
}

impl fmt::Display for ProposalValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongProposer { actual, expected } => {
                write!(f, "Wrong proposer: got {actual}, expected {expected}")
            }
            Self::Signature(err) => {
                write!(f, "Signature verification failed: {err:?}")
            }
            Self::ValidatorSetNotFound { height } => {
                write!(f, "Validator set not found for height {height}")
            }
        }
    }
}

// Make up a seed for the rng based on our address in
// order for each node to likely propose different values at
// each round.
fn seed_from_address(address: &Address) -> u64 {
    address.into_inner().chunks(8).fold(0u64, |acc, chunk| {
        let term = chunk.iter().fold(0u64, |acc, &x| {
            acc.wrapping_shl(8).wrapping_add(u64::from(x))
        });
        acc.wrapping_add(term)
    })
}

fn build_execution_block_from_bytes(raw_block_data: Bytes) -> ExecutionBlock {
    let execution_payload: ExecutionPayloadV3 = ExecutionPayloadV3::from_ssz_bytes(&raw_block_data)
        .expect("failed to convert block bytes into executon payload");
    ExecutionBlock {
        block_hash: execution_payload.payload_inner.payload_inner.block_hash,
        block_number: execution_payload.payload_inner.payload_inner.block_number,
        parent_hash: execution_payload.payload_inner.payload_inner.parent_hash,
        timestamp: execution_payload.payload_inner.payload_inner.timestamp,
        prev_randao: execution_payload.payload_inner.payload_inner.prev_randao,
    }
}

impl State {
    /// Creates a new State instance with the given validator address and starting height
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        _genesis: Genesis, // all genesis data is in EVM via genesis.json
        ctx: EmeraldContext,
        signing_provider: K256Provider,
        address: Address,
        height: Height,
        store: Store,
        state_metrics: StateMetrics,
        num_certificates_to_retain: u64,
        prune_at_interval: u64,
        num_temp_blocks_retained: u64,
        min_block_time: Duration,
        eth_chain_config: ChainConfig,
    ) -> Self {
        // Calculate start_time by subtracting elapsed_seconds from now.
        // It represents the start time of measuring metrics, not the actual node start time.
        // This allows us to continue accumulating time correctly after a restart
        let start_time =
            Instant::now() - core::time::Duration::from_secs(state_metrics.elapsed_seconds);

        Self {
            ctx,
            signing_provider,
            consensus_height: height,
            consensus_round: Round::new(0),
            address,
            store,
            stream_nonce: 0,
            streams_map: PartStreamsMap::new(),
            rng: StdRng::seed_from_u64(seed_from_address(&address)),

            latest_block: None,
            validator_set: None,

            validated_payload_cache: ValidatedPayloadCache::new(10),

            txs_count: state_metrics.txs_count,
            chain_bytes: state_metrics.chain_bytes,
            start_time,
            metrics: state_metrics.metrics,
            num_certificates_to_retain,
            prune_at_block_interval: prune_at_interval,
            num_temp_blocks_retained,
            min_block_time,
            last_block_time: Instant::now(),
            previous_block_commit_time: Instant::now(),
            eth_chain_config,
        }
    }

    pub fn get_fork(&self, block_timestamp: BlockTimestamp) -> Fork {
        let is_osaka = self
            .eth_chain_config
            .osaka_time
            .is_some_and(|time| time <= block_timestamp);
        if is_osaka {
            return Fork::Osaka;
        }
        let is_prague = self
            .eth_chain_config
            .prague_time
            .is_some_and(|time| time <= block_timestamp);
        if is_prague {
            return Fork::Prague;
        }
        Fork::Unsupported
    }

    pub fn validated_cache_mut(&mut self) -> &mut ValidatedPayloadCache {
        &mut self.validated_payload_cache
    }

    pub async fn get_latest_block_candidate(&self, height: Height) -> Option<ExecutionBlock> {
        let decided_value = self.store.get_decided_value(height).await.ok().flatten()?;

        let certificate = decided_value.certificate;

        let raw_block_data = self
            .get_block_data(certificate.height, certificate.round, certificate.value_id)
            .await
            .expect("state: certificate should have associated block data");
        debug!(
            "🎁 block size: {:?}, height: {}",
            raw_block_data.iter().len(),
            height
        );
        Some(build_execution_block_from_bytes(raw_block_data))
    }

    /// Returns the earliest height available via EL
    pub async fn get_earliest_height(&self) -> Height {
        self.store
            .min_decided_value_height()
            .await
            .unwrap_or_default()
    }

    /// Returns the earliest height available in the state
    pub async fn get_earliest_unpruned_height(&self) -> Height {
        self.store
            .min_unpruned_decided_value_height()
            .await
            .unwrap_or_default()
    }

    /// Validates a proposal by checking both proposer and signature
    pub fn validate_proposal_parts(
        &self,
        parts: &ProposalParts,
    ) -> Result<(), ProposalValidationError> {
        let height = parts.height;
        let round = parts.round;

        // Get the expected proposer for this height and round
        let validator_set = self
            .get_validator_set(height)
            .ok_or(ProposalValidationError::ValidatorSetNotFound { height })?;
        let expected_proposer = self
            .ctx
            .select_proposer(validator_set, height, round)
            .address;

        // Check if the proposer matches the expected proposer
        if parts.proposer != expected_proposer {
            return Err(ProposalValidationError::WrongProposer {
                actual: parts.proposer,
                expected: expected_proposer,
            });
        }

        // If proposer is correct, verify the signature
        self.verify_proposal_parts_signature(parts)
            .map_err(ProposalValidationError::Signature)?;

        Ok(())
    }

    /// Verify proposal signature
    fn verify_proposal_parts_signature(
        &self,
        parts: &ProposalParts,
    ) -> Result<(), SignatureVerificationError> {
        let mut hasher = sha3::Keccak256::new();

        let init = parts
            .init()
            .ok_or(SignatureVerificationError::MissingInitPart)?;

        let fin = parts
            .fin()
            .ok_or(SignatureVerificationError::MissingFinPart)?;

        let hash = {
            hasher.update(init.height.as_u64().to_be_bytes());
            hasher.update(init.round.as_i64().to_be_bytes());

            // The correctness of the hash computation relies on the parts being ordered by sequence
            // number, which is guaranteed by the `PartStreamsMap`.
            for part in parts.parts.iter().filter_map(|part| part.as_data()) {
                hasher.update(part.bytes.as_ref());
            }

            hasher.finalize()
        };

        // Retrieve the proposer from the validator set for the given height
        let validator_set = self.get_validator_set(parts.height).ok_or(
            SignatureVerificationError::ValidatorSetNotFound {
                _height: parts.height,
            },
        )?;
        let proposer = validator_set
            .get_by_address(&parts.proposer)
            .ok_or(SignatureVerificationError::ProposerNotFound)?;

        // Verify the signature
        if !self
            .signing_provider
            .verify(&hash, &fin.signature, &proposer.public_key)
        {
            return Err(SignatureVerificationError::InvalidSignature);
        }

        Ok(())
    }

    /// Processes complete proposal parts: validates, stores, and returns the proposed value.
    ///
    /// Returns `Ok(Some(ProposedValue))` if the proposal is valid and stored,
    /// `Ok(None)` if validation fails, or an error for storage/engine failures.
    pub async fn process_complete_proposal_parts(
        &mut self,
        parts: &ProposalParts,
        engine: &Engine,
        retry_config: &RetryConfig,
    ) -> eyre::Result<Option<ProposedValue<EmeraldContext>>> {
        // Validate proposal (proposer + signature)
        if let Err(error) = self.validate_proposal_parts(parts) {
            error!(
                height = %parts.height,
                round = %parts.round,
                proposer = %parts.proposer,
                error = ?error,
                "Rejecting invalid proposal"
            );
            return Ok(None);
        }

        // Assemble the proposal from its parts
        let (value, data) = assemble_value_from_parts(parts.clone());

        // Log first 32 bytes of proposal data and total size
        info!(
            data = %hex::encode(&data[..data.len().min(32)]),
            total_size = %data.len(),
            id = %value.value.id().as_u64(),
            "Proposal data"
        );

        // Validate the execution payload with the execution engine
        let validity = validate_execution_payload(
            &mut self.validated_payload_cache,
            &data,
            value.height,
            value.round,
            engine,
            retry_config,
        )
        .await?;

        if validity == Validity::Invalid {
            warn!(
                height = %parts.height,
                round = %parts.round,
                "Proposal has invalid execution payload, rejecting"
            );
            return Ok(None);
        }

        // Store as undecided
        info!(%value.height, %value.round, %value.proposer, "Storing validated proposal as undecided");
        self.store_undecided_value(&value, data).await?;

        Ok(Some(value))
    }

    /// Reassembles proposal parts from streamed messages.
    ///
    /// Handles height filtering:
    /// - Outdated proposals (height < current) are dropped
    /// - Future proposals (height > current) are stored as pending
    /// - Current height proposals are returned for validation
    ///
    /// Returns `Some(ProposalParts)` when a complete proposal is ready for validation,
    /// `None` if the proposal is incomplete, outdated, or stored for later.
    pub async fn reassemble_proposal(
        &mut self,
        from: PeerId,
        part: StreamMessage<ProposalPart>,
    ) -> eyre::Result<Option<ProposalParts>> {
        let sequence = part.sequence;

        // Check if we have a full proposal
        let Some(parts) = self.streams_map.insert(from, part) else {
            return Ok(None);
        };

        // Check if the proposal is outdated
        if parts.height < self.consensus_height {
            debug!(
                height = %self.consensus_height,
                round = %self.consensus_round,
                part.height = %parts.height,
                part.round = %parts.round,
                part.sequence = %sequence,
                "Received outdated proposal part, ignoring"
            );

            return Ok(None);
        }

        // Store future proposals parts in pending without validation
        if parts.height > self.consensus_height {
            info!(%parts.height, %parts.round, "Storing proposal parts for a future height in pending");
            self.store.store_pending_proposal_parts(parts).await?;
            return Ok(None);
        }

        // For current height, return parts for validation
        Ok(Some(parts))
    }

    /// Retrieves a decided block data at the given height
    pub async fn get_block_data(
        &self,
        height: Height,
        round: Round,
        value_id: ValueId,
    ) -> Option<Bytes> {
        self.store
            .get_block_data(height, round, value_id)
            .await
            .ok()
            .flatten()
    }

    /// Stores an undecided proposal along with its block data.
    ///
    /// WARN: The order of the two storage operations is important.
    /// Block data must be stored before the proposal metadata to prevent crashes from
    /// leaving a proposal that references non-existent block data. If a crash occurs
    /// between the operations, orphaned block data is safe, but a dangling proposal
    /// reference would cause retrieval failures.
    pub async fn store_undecided_value(
        &self,
        value: &ProposedValue<EmeraldContext>,
        data: Bytes,
    ) -> eyre::Result<()> {
        self.store
            .store_undecided_block_data(value.height, value.round, value.value.id(), data)
            .await?;
        self.store.store_undecided_proposal(value.clone()).await?;
        Ok(())
    }

    /// Commits a value with the given certificate, updating internal state
    /// and moving to the next height
    pub async fn commit(
        &mut self,
        certificate: CommitCertificate<EmeraldContext>,
    ) -> eyre::Result<()> {
        info!(
            height = %certificate.height,
            round = %certificate.round,
            "Looking for certificate"
        );

        let proposal = self
            .store
            .get_undecided_proposal(certificate.height, certificate.round, certificate.value_id)
            .await;

        let proposal = match proposal {
            Ok(Some(proposal)) => proposal,
            Ok(None) => {
                error!(
                    height = %certificate.height,
                    round = %certificate.round,
                    "Trying to commit a value that is not decided"
                );

                return Ok(()); // FIXME: Return an actual error and handle in caller
            }
            Err(e) => return Err(e.into()),
        };

        // Get block data for decided value
        let block_data = self
            .store
            .get_block_data(certificate.height, certificate.round, certificate.value_id)
            .await?;

        // Log first 32 bytes of block data with JNT prefix
        if let Some(data) = &block_data {
            if data.len() >= 32 {
                info!("Committed block_data[0..32]: {}", hex::encode(&data[..32]));
            }
        }

        if let Some(data) = block_data {
            // Store decided value and the block header
            let execution_payload = ExecutionPayloadV3::from_ssz_bytes(&data).unwrap();
            let block_header = extract_block_header(&execution_payload);
            let block_header_bytes = Bytes::from(block_header.as_ssz_bytes());
            self.store
                .store_decided_value(&certificate, proposal.value, block_header_bytes)
                .await?;

            // Store decided block data
            self.store
                .store_decided_block_data(certificate.height, data)
                .await?;
        }

        let prune_certificates = self.num_certificates_to_retain != u64::MAX
            && certificate.height.as_u64() % self.prune_at_block_interval == 0;

        //        Prune only if the current height is above the minimum block retain height
        if certificate.height >= Height::new(self.num_temp_blocks_retained) {
            // If storege becomes a bottleneck, consider optimizing this by pruning every INTERVAL heights
            self.store
                .prune(
                    self.num_certificates_to_retain,
                    self.num_temp_blocks_retained,
                    certificate.height,
                    prune_certificates,
                )
                .await?;
        }
        // Sleep to reduce the block speed, if set via config.
        debug!(timeout_commit = ?self.min_block_time);
        let elapsed_height_time = self.last_block_time.elapsed();

        info!(
            "👉 stats at {:?}: block_time {:?}",
            certificate.height, elapsed_height_time
        );

        if elapsed_height_time < self.min_block_time {
            tokio::time::sleep(self.min_block_time - elapsed_height_time).await;
        }

        Ok(())
    }

    /// Retrieves a previously built proposal value for the given height and round.
    /// Called by the consensus engine to re-use a previously built value.
    /// There should be at most one proposal for a given height and round when the proposer is not byzantine.
    /// We assume this implementation is not byzantine and we are the proposer for the given height and round.
    /// Therefore there must be a single proposal for the rounds where we are the proposer, with the proposer address matching our own.
    pub async fn get_previously_built_value(
        &self,
        height: Height,
        round: Round,
    ) -> eyre::Result<Option<LocallyProposedValue<EmeraldContext>>> {
        let proposals: Vec<ProposedValue<EmeraldContext>> =
            self.store.get_undecided_proposals(height, round).await?;

        assert!(
            proposals.len() <= 1,
            "There should be at most one proposal for a given height and round"
        );

        proposals
            .first()
            .map(|p| LocallyProposedValue::new(p.height, p.round, p.value.clone()))
            .map(Some)
            .map(Ok)
            .unwrap_or(Ok(None))
    }

    /// Retrieves a previously built proposal value for the given height and round.
    /// Called by the consensus engine to re-use a previously built value.
    /// There should be at most one proposal for a given height and round when the proposer is not byzantine.
    /// We assume this implementation is not byzantine and we are the proposer for the given height and round.
    /// Therefore there must be a single proposal for the rounds where we are the proposer, with the proposer address matching our own.
    pub async fn get_previous_proposal_by_value_and_proposer(
        &self,
        height: Height,
        round: Round,
        value_id: ValueId,
        address: Address,
    ) -> eyre::Result<Option<LocallyProposedValue<EmeraldContext>>> {
        let proposal = self
            .store
            .get_undecided_proposal(height, round, value_id)
            .await?;
        match proposal {
            Some(prop) => {
                if prop.proposer.eq(&address) {
                    let lp: LocallyProposedValue<EmeraldContext> =
                        LocallyProposedValue::new(prop.height, prop.round, prop.value);
                    Ok(Some(lp))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    // /// Make up a new value to propose
    // /// A real application would have a more complex logic here,
    // /// typically reaping transactions from a mempool and executing them against its state,
    // /// before computing the merkle root of the new app state.
    // fn make_value(&mut self) -> Value {
    //     let value = self.rng.gen_range(100..=100000);
    //     Value::new(value)
    // }

    #[allow(dead_code)]
    pub fn make_block(&mut self) -> Bytes {
        let mut random_bytes = vec![0u8; BLOCK_SIZE];
        self.rng.fill(&mut random_bytes[..]);
        Bytes::from(random_bytes)
    }

    /// Creates a new proposal value for the given height
    /// Returns either a previously built proposal or creates a new one
    pub async fn propose_value(
        &mut self,
        height: Height,
        round: Round,
        data: Bytes,
    ) -> eyre::Result<LocallyProposedValue<EmeraldContext>> {
        assert_eq!(height, self.consensus_height);
        assert_eq!(round, self.consensus_round);

        // We create a new value.
        let value = Value::new(data.clone());

        let proposal: ProposedValue<EmeraldContext> = ProposedValue {
            height,
            round,
            valid_round: Round::Nil,
            proposer: self.address, // We are the proposer
            value,
            validity: Validity::Valid, // Our proposals are de facto valid
        };

        // Store the proposal and its block data
        self.store_undecided_value(&proposal, data).await?;

        Ok(LocallyProposedValue::new(
            proposal.height,
            proposal.round,
            proposal.value,
        ))
    }

    fn stream_id(&mut self) -> StreamId {
        let mut bytes = Vec::with_capacity(size_of::<u64>() + size_of::<u32>());
        bytes.extend_from_slice(&self.consensus_height.as_u64().to_be_bytes());
        bytes.extend_from_slice(&self.consensus_round.as_u32().unwrap().to_be_bytes());
        bytes.extend_from_slice(&self.stream_nonce.to_be_bytes());
        self.stream_nonce += 1;
        StreamId::new(bytes.into())
    }

    /// Creates a stream message containing a proposal part.
    /// Updates internal sequence number and current proposal.
    pub fn stream_proposal(
        &mut self,
        value: LocallyProposedValue<EmeraldContext>,
        data: Bytes,
        pol_round: Round,
    ) -> impl Iterator<Item = StreamMessage<ProposalPart>> {
        let parts = self.make_proposal_parts(value, data, pol_round);

        let stream_id = self.stream_id();

        let mut msgs = Vec::with_capacity(parts.len() + 1);
        let mut sequence = 0;

        for part in parts {
            let msg = StreamMessage::new(stream_id.clone(), sequence, StreamContent::Data(part));
            sequence += 1;
            msgs.push(msg);
        }

        msgs.push(StreamMessage::new(stream_id, sequence, StreamContent::Fin));
        msgs.into_iter()
    }

    fn make_proposal_parts(
        &self,
        value: LocallyProposedValue<EmeraldContext>,
        data: Bytes,
        pol_round: Round,
    ) -> Vec<ProposalPart> {
        let mut hasher = sha3::Keccak256::new();
        let mut parts = Vec::new();

        // Init
        {
            parts.push(ProposalPart::Init(ProposalInit::new(
                value.height,
                value.round,
                pol_round,
                self.address,
            )));

            hasher.update(value.height.as_u64().to_be_bytes().as_slice());
            hasher.update(value.round.as_i64().to_be_bytes().as_slice());
        }

        // Data
        {
            for chunk in data.chunks(CHUNK_SIZE) {
                let chunk_data = ProposalData::new(Bytes::copy_from_slice(chunk));
                parts.push(ProposalPart::Data(chunk_data));
                hasher.update(chunk);
            }
        }

        {
            let hash = hasher.finalize().to_vec();
            let signature = self.signing_provider.sign(&hash);
            parts.push(ProposalPart::Fin(ProposalFin::new(signature)));
        }

        parts
    }

    /// Returns the set of validators for the given consensus height.
    /// Returns None if the height doesn't match the stored validator set height.
    pub fn get_validator_set(&self, height: Height) -> Option<&ValidatorSet> {
        self.validator_set
            .as_ref()
            .and_then(|(h, vs)| if *h == height { Some(vs) } else { None })
    }

    /// Sets the validator set for the given consensus height.
    pub fn set_validator_set(&mut self, height: Height, validator_set: ValidatorSet) {
        self.validator_set = Some((height, validator_set));
    }

    /// Update and log per-block statistics
    pub async fn log_block_stats(
        &mut self,
        height: Height,
        tx_count: usize,
        block_bytes_len: usize,
        block_time_secs: f64,
    ) -> eyre::Result<()> {
        // Calculate per-block metrics
        let txs_per_second = if block_time_secs > 0.0 {
            tx_count as f64 / block_time_secs
        } else {
            0.0
        };
        let bytes_per_second = if block_time_secs > 0.0 {
            block_bytes_len as f64 / block_time_secs
        } else {
            0.0
        };

        // Update cumulative counters
        self.txs_count += tx_count as u64;
        self.chain_bytes += block_bytes_len as u64;
        let elapsed_time = self.start_time.elapsed();

        // Update metrics
        self.metrics.tx_stats.add_txs(tx_count as u64);
        self.metrics
            .tx_stats
            .add_chain_bytes(block_bytes_len as u64);
        self.metrics.tx_stats.set_txs_per_second(txs_per_second);
        self.metrics.tx_stats.set_bytes_per_second(bytes_per_second);
        self.metrics.tx_stats.set_block_tx_count(tx_count as u64);
        self.metrics.tx_stats.set_block_size(block_bytes_len as u64);

        // Persist cumulative metrics to database for crash recovery
        self.store
            .store_cumulative_metrics(self.txs_count, self.chain_bytes, elapsed_time.as_secs())
            .await?;

        info!(
            "👉 stats at height {}: block_time={:.3}s, #txs={}, txs/s={:.2}, block_bytes={}, bytes/s={:.2}, total_txs={}, total_bytes={}",
            height,
            block_time_secs,
            tx_count,
            txs_per_second,
            block_bytes_len,
            bytes_per_second,
            self.txs_count,
            self.chain_bytes,
        );

        Ok(())
    }
}

/// Re-assemble a [`ProposedValue`] from its [`ProposalParts`].
///
/// This is done by multiplying all the factors in the parts.
pub fn assemble_value_from_parts(parts: ProposalParts) -> (ProposedValue<EmeraldContext>, Bytes) {
    // Get the init part to extract pol_round
    let init = parts
        .parts
        .iter()
        .find_map(|part| part.as_init())
        .expect("ProposalParts should have an init part");

    // Calculate total size and allocate buffer
    let total_size: usize = parts
        .parts
        .iter()
        .filter_map(|part| part.as_data())
        .map(|data| data.bytes.len())
        .sum();

    let mut data = Vec::with_capacity(total_size);
    // Concatenate all chunks
    for part in parts.parts.iter().filter_map(|part| part.as_data()) {
        data.extend_from_slice(&part.bytes);
    }

    // Convert the concatenated data vector into Bytes
    let data = Bytes::from(data);

    let proposed_value = ProposedValue {
        height: parts.height,
        round: parts.round,
        valid_round: init.pol_round,
        proposer: parts.proposer,
        value: Value::new(data.clone()),
        validity: Validity::Valid,
    };

    (proposed_value, data)
}

/// Decodes a Value from its byte representation using ProtobufCodec
pub fn decode_value(bytes: Bytes) -> Value {
    ProtobufCodec.decode(bytes).unwrap()
}
