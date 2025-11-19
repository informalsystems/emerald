//! Internal state of the application. This is a simplified abstract to keep it simple.
//! A regular application would have mempool implemented, a proper database and input methods like RPC.

use std::fmt;

use alloy_rpc_types_engine::ExecutionPayloadV3;
use bytes::Bytes;
use caches::lru::AdaptiveCache;
use caches::Cache;
use color_eyre::eyre;
use malachitebft_app_channel::app::streaming::{StreamContent, StreamId, StreamMessage};
use malachitebft_app_channel::app::types::codec::Codec;
use malachitebft_app_channel::app::types::core::{CommitCertificate, Context, Round, Validity};
use malachitebft_app_channel::app::types::{LocallyProposedValue, PeerId, ProposedValue};
use malachitebft_eth_engine::engine::Engine;
use malachitebft_eth_engine::json_structures::ExecutionBlock;
use malachitebft_eth_types::codec::proto::ProtobufCodec;
use malachitebft_eth_types::secp256k1::K256Provider;
use malachitebft_eth_types::{
    Address, Block, BlockHash, EmeraldContext, Genesis, Height, ProposalData, ProposalFin,
    ProposalInit, ProposalPart, RetryConfig, ValidatorSet, Value, ValueId,
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use sha3::Digest;
use ssz::Decode;
use tokio::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::metrics::Metrics;
use crate::store::Store;
use crate::streaming::{PartStreamsMap, ProposalParts};

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
use crate::sync_handler::validate_payload;

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
    signing_provider: K256Provider,
    address: Address,
    pub store: Store,
    stream_nonce: u32,
    streams_map: PartStreamsMap,
    #[allow(dead_code)]
    rng: StdRng,

    pub current_height: Height,
    pub current_round: Round,
    pub current_proposer: Option<Address>,

    pub latest_block: Option<ExecutionBlock>,

    validator_set: Option<ValidatorSet>,

    // Cache for tracking recently validated payloads to avoid duplicate validation
    validated_payload_cache: ValidatedPayloadCache,

    // For stats
    pub txs_count: u64,
    pub chain_bytes: u64,
    pub start_time: Instant,
    pub metrics: Metrics,

    pub min_block_time: Duration,

    pub last_block_time: Instant,
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
}

/// Represents errors that can occur during proposal validation.
#[derive(Debug)]
pub enum ProposalValidationError {
    /// Proposer doesn't match the expected proposer for the given round
    WrongProposer { actual: Address, expected: Address },
    /// Signature verification errors
    Signature(SignatureVerificationError),
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
        min_block_time: Duration,
    ) -> Self {
        // Calculate start_time by subtracting elapsed_seconds from now.
        // It represents the start time of measuring metrics, not the actual node start time.
        // This allows us to continue accumulating time correctly after a restart
        let start_time =
            Instant::now() - core::time::Duration::from_secs(state_metrics.elapsed_seconds);

        Self {
            ctx,
            signing_provider,
            current_height: height,
            current_round: Round::new(0),
            current_proposer: None,
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
            min_block_time,
            last_block_time: Instant::now(),
        }
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
        let validator_set = self.get_validator_set(); // TODO: Should pass height as a parameter
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
        let validator_set = self.get_validator_set(); // TODO: Should pass height as a parameter
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

    /// Validates execution payload with the execution engine
    /// Returns Ok(Validity) - Invalid if decoding fails or payload is invalid
    pub async fn validate_execution_payload(
        &mut self,
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
            &mut self.validated_payload_cache,
            engine,
            &execution_payload,
            &versioned_hashes,
            retry_config,
            height,
            round,
        )
        .await
    }

    /// Processes and adds a new proposal to the state if it's valid
    /// Returns Some(ProposedValue) if the proposal was accepted, None otherwise
    pub async fn received_proposal_part(
        &mut self,
        from: PeerId,
        part: StreamMessage<ProposalPart>,
        engine: &Engine,
        retry_config: &RetryConfig,
    ) -> eyre::Result<Option<ProposedValue<EmeraldContext>>> {
        let sequence = part.sequence;

        // Check if we have a full proposal
        let Some(parts) = self.streams_map.insert(from, part) else {
            return Ok(None);
        };

        // Check if the proposal is outdated
        if parts.height < self.current_height {
            debug!(
                height = %self.current_height,
                round = %self.current_round,
                part.height = %parts.height,
                part.round = %parts.round,
                part.sequence = %sequence,
                "Received outdated proposal part, ignoring"
            );

            return Ok(None);
        }

        // Store future proposals parts in pending without validation
        if parts.height > self.current_height {
            info!(%parts.height, %parts.round, "Storing proposal parts for a future height in pending");
            self.store.store_pending_proposal_parts(parts).await?;
            return Ok(None);
        }

        // For current height, validate proposal (proposer + signature)
        match self.validate_proposal_parts(&parts) {
            Ok(()) => {
                // Validation passed - assemble and store as undecided
                // Re-assemble the proposal from its parts
                let (value, data) = assemble_value_from_parts(parts);

                // Log first 32 bytes of proposal data and total size
                if data.len() >= 32 {
                    info!(
                        "Proposal data[0..32]: {}, total_size: {} bytes, id: {:x}",
                        hex::encode(&data[..32]),
                        data.len(),
                        value.value.id().as_u64()
                    );
                }

                // Validate the execution payload with the execution engine
                let validity = self
                    .validate_execution_payload(
                        &data,
                        value.height,
                        value.round,
                        engine,
                        retry_config,
                    )
                    .await?;

                if validity == Validity::Invalid {
                    warn!(
                        height = %self.current_height,
                        round = %self.current_round,
                        "Received proposal with invalid execution payload, ignoring"
                    );
                    return Ok(None);
                }
                info!(%value.height, %value.round, %value.proposer, "Storing validated proposal as undecided");
                self.store_undecided_block_data(value.height, value.round, value.value.id(), data)
                    .await?;
                self.store.store_undecided_proposal(value.clone()).await?;

                Ok(Some(value))
            }
            Err(error) => {
                // Any validation error indicates invalid proposal - log and reject
                error!(
                    height = %parts.height,
                    round = %parts.round,
                    proposer = %parts.proposer,
                    error = ?error,
                    "Rejecting invalid proposal"
                );
                Ok(None)
            }
        }
    }
    pub async fn store_undecided_block_data(
        &mut self,
        height: Height,
        round: Round,
        value_id: ValueId,
        data: Bytes,
    ) -> eyre::Result<()> {
        self.store
            .store_undecided_block_data(height, round, value_id, data)
            .await
            .map_err(|e| eyre::Report::new(e))
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

    /// Commits a value with the given certificate, updating internal state
    /// and moving to the next height
    pub async fn commit(
        &mut self,
        certificate: CommitCertificate<EmeraldContext>,
        block_header_bytes: Bytes,
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

        self.store
            .store_decided_value(&certificate, proposal.value, block_header_bytes)
            .await?;

        // Store block data for decided value
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
            self.store
                .store_decided_block_data(certificate.height, data)
                .await?;
        }

        // Prune the store, keep the last 5 heights
        let retain_height = Height::new(certificate.height.as_u64().saturating_sub(5));
        self.store.prune(retain_height).await?;

        // Move to next height
        self.current_height = self.current_height.increment();
        self.current_round = Round::new(0);

        // Sleep to reduce the block speed, if set via config.
        info!("timeout commit is {:?}", self.min_block_time);
        let elapsed_height_time = self.last_block_time.elapsed();

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
        let proposals = self.store.get_undecided_proposals(height, round).await?;

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
        assert_eq!(height, self.current_height);
        assert_eq!(round, self.current_round);

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
        // Store the block data at the proposal's height/round,
        // which will be passed to the execution client (EL) on commit.
        // WARN: THE ORDER OF THE FOLLOWING TWO OPERATIONS IS IMPORTANT.
        self.store_undecided_block_data(height, round, proposal.value.id(), data.clone())
            .await?;

        // Insert the new proposal into the undecided proposals.
        self.store
            .store_undecided_proposal(proposal.clone())
            .await?;

        Ok(LocallyProposedValue::new(
            proposal.height,
            proposal.round,
            proposal.value,
        ))
    }

    fn stream_id(&mut self) -> StreamId {
        let mut bytes = Vec::with_capacity(size_of::<u64>() + size_of::<u32>());
        bytes.extend_from_slice(&self.current_height.as_u64().to_be_bytes());
        bytes.extend_from_slice(&self.current_round.as_u32().unwrap().to_be_bytes());
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

    /// Returns the set of validators.
    pub fn get_validator_set(&self) -> &ValidatorSet {
        self.validator_set
            .as_ref()
            .expect("Validator set must be initialized before use")
    }

    /// Sets the validator set.
    pub fn set_validator_set(&mut self, validator_set: ValidatorSet) {
        self.validator_set = Some(validator_set);
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

/// Extracts a block header from an ExecutionPayloadV3 by removing transactions and withdrawals.
///
/// Returns an ExecutionPayloadV3 with empty transactions and withdrawals vectors,
/// containing only the block header fields.
pub fn extract_block_header(
    payload: &alloy_rpc_types_engine::ExecutionPayloadV3,
) -> alloy_rpc_types_engine::ExecutionPayloadV3 {
    use alloy_rpc_types_engine::{ExecutionPayloadV1, ExecutionPayloadV2, ExecutionPayloadV3};

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
    header: alloy_rpc_types_engine::ExecutionPayloadV3,
    body: malachitebft_eth_engine::json_structures::ExecutionPayloadBodyV1,
) -> alloy_rpc_types_engine::ExecutionPayloadV3 {
    use alloy_rpc_types_engine::{ExecutionPayloadV1, ExecutionPayloadV2, ExecutionPayloadV3};

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
