use alloy_primitives::{address, Address};
use alloy_provider::ProviderBuilder;
use alloy_rpc_types_engine::{ExecutionPayloadV3, PayloadStatusEnum};
use bytes::Bytes;
use color_eyre::eyre::{self, eyre, OptionExt};
use malachitebft_app_channel::app::engine::host::Next;
use malachitebft_app_channel::app::streaming::StreamContent;
use malachitebft_app_channel::app::types::core::{Round, Validity};
use malachitebft_app_channel::app::types::{LocallyProposedValue, ProposedValue};
use malachitebft_app_channel::{AppMsg, Channels, NetworkMsg};
use malachitebft_eth_cli::config::EmeraldConfig;
use malachitebft_eth_engine::engine::Engine;
use malachitebft_eth_engine::json_structures::ExecutionBlock;
use malachitebft_eth_types::secp256k1::PublicKey;
use malachitebft_eth_types::{Block, BlockHash, EmeraldContext, Height, Validator, ValidatorSet};
use ssz::{Decode, Encode};
use tokio::time::Instant;
use tracing::{debug, error, info, warn};

const GENESIS_VALIDATOR_MANAGER_ACCOUNT: Address =
    address!("0x0000000000000000000000000000000000002000");

alloy_sol_types::sol!(
    #[derive(Debug)]
    #[sol(rpc)]
    ValidatorManager,
    "../solidity/out/ValidatorManager.sol/ValidatorManager.json"
);

use crate::state::{assemble_value_from_parts, decode_value, State};
use crate::sync_handler::{get_decided_value_for_sync, validate_payload};

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
    state: &State,
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

        // Get the certificate and header from store
        let (_certificate, header_bytes) = state
            .store
            .get_certificate_and_header(height)
            .await?
            .ok_or_eyre(format!("Missing certificate or header for height {height}"))?;

        // Deserialize the execution payload
        let execution_payload = ExecutionPayloadV3::from_ssz_bytes(&header_bytes).map_err(|e| {
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
            replay_heights_to_engine(state, engine, replay_start, height, emerald_config).await?;

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
            replay_heights_to_engine(state, engine, Height::new(1), height, emerald_config).await?;
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

pub async fn read_validators_from_contract(
    eth_url: &str,
    block_hash: &BlockHash,
) -> eyre::Result<ValidatorSet> {
    let provider = ProviderBuilder::new().connect(eth_url).await?;

    let validator_manager_contract =
        ValidatorManager::new(GENESIS_VALIDATOR_MANAGER_ACCOUNT, provider);

    let genesis_validator_set_sol = validator_manager_contract
        .getValidators()
        .block((*block_hash).into())
        .call()
        .await?;

    let validators = genesis_validator_set_sol
        .into_iter()
        .map(
            |ValidatorManager::ValidatorInfo {
                 validatorKey,
                 power,
             }| {
                let mut uncompressed = [0u8; 65];
                uncompressed[0] = 0x04;
                uncompressed[1..33].copy_from_slice(&validatorKey.x.to_be_bytes::<32>());
                uncompressed[33..].copy_from_slice(&validatorKey.y.to_be_bytes::<32>());

                let pub_key = PublicKey::from_sec1_bytes(&uncompressed)?;

                Ok(Validator::new(pub_key, power))
            },
        )
        .collect::<eyre::Result<Vec<_>>>()?;

    Ok(ValidatorSet::new(validators))
}

/// Handle ConsensusReady messages from the consensus engine
///
/// Notifies the application that consensus is ready.
///
/// The application MUST reply with a message to instruct
/// consensus to start at a given height.
pub async fn on_consensus_ready(
    consensus_ready: AppMsg<EmeraldContext>,
    state: &mut State,
    engine: &Engine,
    emerald_config: &EmeraldConfig,
) -> eyre::Result<()> {
    let AppMsg::ConsensusReady { reply } = consensus_ready else {
        unreachable!("on_consensus_ready called with non-ConsensusReady message");
    };

    info!("🟢🟢 Consensus is ready");

    // Node start-up: https://hackmd.io/@danielrachi/engine_api#Node-startup
    // Check compatibility with execution client
    engine.check_capabilities().await?;

    // Get latest decided height from local store
    let latest_height_from_store = state.store.max_decided_value_height().await;
    match latest_height_from_store {
        Some(h) => {
            initialize_state_from_existing_block(state, engine, h, emerald_config).await?;
            info!(
                "Starting from existing block at height {:?}. Current tip (consensus height): {:?} ",
                h,
                state.consensus_height
            );
        }
        None => {
            // Get the genesis block from the execution engine
            initialize_state_from_genesis(state, engine).await?;
            info!(
                "Starting from genesis. Current tip (consensus height): {:?}",
                state.consensus_height
            );
        }
    }

    // We can simply respond by telling the engine to start consensus
    // at consensus_height (which tracks the tip where consensus will work)
    if reply
        .send((
            state.consensus_height,
            state
                .get_validator_set(state.consensus_height)
                .ok_or_eyre(format!(
                    "Validator set not found for height {}",
                    state.consensus_height
                ))?
                .clone(),
        ))
        .is_err()
    {
        error!("Failed to send ConsensusReady reply");
    }

    Ok(())
}

/// Handle StartedRound messages from the consensus engine
///
/// Notifies the application that a new consensus round has begun.
pub async fn on_started_round(
    started_round: AppMsg<EmeraldContext>,
    state: &mut State,
    engine: &Engine,
    emerald_config: &EmeraldConfig,
) -> eyre::Result<()> {
    let AppMsg::StartedRound {
        height,
        round,
        proposer,
        role,
        reply_value,
    } = started_round
    else {
        unreachable!("on_started_round called with non-StartedRound message");
    };

    info!(%height, %round, %proposer, ?role, "🟢🟢 Started round");

    // The consensus_height stored in state should match
    // the one in the StartedRound message
    if state.consensus_height != height {
        warn!(
            consensus_height = %state.consensus_height,
            new_height = %height,
            "Started round mismatch between state and message"
        );
    }

    // We can use that opportunity to update our internal state
    state.consensus_height = height;
    state.consensus_round = round;

    if state.consensus_round == Round::ZERO {
        state.last_block_time = Instant::now();
    }

    let pending_parts = state
        .store
        .get_pending_proposal_parts(height, round)
        .await?;
    debug!(
        %height,
        %round,
        "Found {} pending proposal parts, validating...",
        pending_parts.len()
    );

    for parts in &pending_parts {
        match state.validate_proposal_parts(parts) {
            Ok(()) => {
                // Validate execution payload with the execution engine before storing it as undecided proposal
                let (value, data) = assemble_value_from_parts(parts.clone());

                let validity = state
                    .validate_execution_payload(
                        &data,
                        parts.height,
                        parts.round,
                        engine,
                        &emerald_config.retry_config,
                    )
                    .await?;

                if validity == Validity::Invalid {
                    warn!(
                        height = %parts.height,
                        round = %parts.round,
                        "Pending proposal has invalid execution payload, rejecting"
                    );
                    continue;
                }

                state.store.store_undecided_proposal(value.clone()).await?;

                state
                    .store
                    .store_undecided_block_data(value.height, value.round, value.value.id(), data)
                    .await?;
                info!(
                    height = %parts.height,
                    round = %parts.round,
                    proposer = %parts.proposer,
                    "Moved valid pending proposal to undecided after validation"
                );
            }
            Err(error) => {
                // Validation failed, log error
                error!(
                    height = %parts.height,
                    round = %parts.round,
                    proposer = %parts.proposer,
                    error = ?error,
                    "Removed invalid pending proposal"
                );
            }
        } // Remove the parts from pending
        state
            .store
            .remove_pending_proposal_parts(parts.clone())
            .await?;
    }

    // If we have already built or seen values for this height and round,
    // send them all back to consensus. This may happen when we are restarting after a crash.
    let proposals = state.store.get_undecided_proposals(height, round).await?;
    debug!(%height, %round, "Found {} undecided proposals", proposals.len());

    if reply_value.send(proposals).is_err() {
        error!("Failed to send undecided proposals");
    }

    Ok(())
}

/// Handle GetValue messages from the consensus engine
///
/// Requests the application to build a value for consensus to propose.
///
/// The application MUST reply to this message with the requested value
/// within the specified timeout duration.
pub async fn on_get_value(
    get_value: AppMsg<EmeraldContext>,
    state: &mut State,
    channels: &Channels<EmeraldContext>,
    engine: &Engine,
    emerald_config: &EmeraldConfig,
) -> eyre::Result<()> {
    let AppMsg::GetValue {
        height,
        round,
        timeout,
        reply,
    } = get_value
    else {
        unreachable!("on_get_value called with non-GetValue message");
    };

    // NOTE: We can ignore the timeout as we are building the value right away.
    // If we were let's say reaping as many txes from a mempool and executing them,
    // then we would need to respect the timeout and stop at a certain point.

    info!(%height, %round, "🟢🟢 Consensus is requesting a value to propose");

    // Here it is important that, if we have previously built a value for this height and round,
    // we send back the very same value.
    let (proposal, bytes) = match state.get_previously_built_value(height, round).await? {
        Some(proposal) => {
            info!(value = %proposal.value.id(), "Re-using previously built value");
            // Fetch the block data for the previously built value
            let bytes = state
                .store
                .get_block_data(height, round, proposal.value.id())
                .await?
                .ok_or_else(|| eyre!("Block data not found for previously built value"))?;
            (proposal, bytes)
        }
        None => {
            // Check if the execution client is syncing and behind the consensus height
            let (is_syncing, highest_chain_height) = engine.is_syncing().await?;
            if is_syncing && highest_chain_height >= height.as_u64() {
                warn!(
                                    "⚠️  Execution client is syncing (current: {}, target: {}), waiting for timeout",
                                    highest_chain_height,
                                    height.as_u64()
                                );
                tokio::time::sleep(timeout * 2).await; // Sleep long enough to trigger timeout_propose
                return Ok(());
            } else {
                // If we have not previously built a value for that very same height and round,
                // we need to create a new value to propose and send it back to consensus.
                info!("Building a new value to propose");
                // We need to ask the execution engine for a new value to
                // propose. Then we send it back to consensus.

                let latest_block = state.latest_block.expect("Head block hash is not set");

                let execution_payload = engine
                    .generate_block(
                        &Some(latest_block),
                        &emerald_config.retry_config,
                        &emerald_config.fee_recipient,
                        state.get_fork(latest_block.timestamp),
                    )
                    .await?;

                debug!("🌈 Got execution payload: {:?}", execution_payload);

                // Store block in state and propagate to peers.
                let bytes = Bytes::from(execution_payload.as_ssz_bytes());
                debug!("🎁 block size: {:?}, height: {}", bytes.len(), height);

                // Prepare block proposal.
                let proposal: LocallyProposedValue<EmeraldContext> =
                    state.propose_value(height, round, bytes.clone()).await?;

                (proposal, bytes)
            }
        }
    };

    // Send it to consensus
    if reply.send(proposal.clone()).is_err() {
        error!("Failed to send GetValue reply");
    }

    // The POL round is always nil when we propose a newly built value.
    // See L15/L18 of the Tendermint algorithm.
    let pol_round = Round::Nil;
    // Now what's left to do is to break down the value to propose into parts,
    // and send those parts over the network to our peers, for them to re-assemble the full value.
    for stream_message in state.stream_proposal(proposal, bytes, pol_round) {
        debug!(%height, %round, "Streaming proposal part: {stream_message:?}");
        channels
            .network
            .send(NetworkMsg::PublishProposalPart(stream_message))
            .await?;
    }
    debug!(%height, %round, "✅ Proposal sent");

    Ok(())
}

/// Handle ReceivedProposalPart messages from the consensus engine
///
/// Notifies the application that consensus has received a proposal part over the network.
///
/// If this part completes the full proposal, the application MUST respond
/// with the complete proposed value. Otherwise, it MUST respond with `None`.
pub async fn on_received_proposal_part(
    received_proposal_part: AppMsg<EmeraldContext>,
    state: &mut State,
    engine: &Engine,
    emerald_config: &EmeraldConfig,
) -> eyre::Result<()> {
    let AppMsg::ReceivedProposalPart { from, part, reply } = received_proposal_part else {
        unreachable!("on_received_proposal_part called with non-ReceivedProposalPart message");
    };

    let (part_type, part_size) = match &part.content {
        StreamContent::Data(part) => (part.get_type(), part.size_bytes()),
        StreamContent::Fin => ("end of stream", 0),
    };

    debug!(
        %from, %part.sequence, part.type = %part_type, part.size = %part_size,
        "Received proposal part"
    );

    // Try to reassemble the proposal from received parts. If present,
    // validate it with the execution engine and mark invalid when
    // parsing or validation fails. Keep the outer `Option` and send it
    // back to the caller (consensus) regardless.
    let proposed_value = state
        .received_proposal_part(from, part, engine, &emerald_config.retry_config)
        .await?;

    if let Some(proposed_value) = proposed_value.clone() {
        debug!("✅ Received complete proposal: {:?}", proposed_value);
    }

    if reply.send(proposed_value).is_err() {
        error!("Failed to send ReceivedProposalPart reply");
    }

    Ok(())
}

/// Handle Decided messages from the consensus engine
///
/// Notifies the application that consensus has decided on a value.
///
/// This message includes a commit certificate containing the ID of
/// the value that was decided on, the height and round at which it was decided,
/// and the aggregated signatures of the validators that committed to it.
/// It also includes to the vote extensions received for that height.
///
/// In response to this message, the application MUST send a [`Next`]
/// message back to consensus, instructing it to either start the next height if
/// the application was able to commit the decided value, or to restart the current height
/// otherwise.
///
/// If the application does not reply, consensus will stall.
pub async fn on_decided(
    decided: AppMsg<EmeraldContext>,
    state: &mut State,
    engine: &Engine,
    emerald_config: &EmeraldConfig,
) -> eyre::Result<()> {
    let AppMsg::Decided {
        certificate, reply, ..
    } = decided
    else {
        unreachable!("on_decided called with non-Decided message");
    };

    let height = certificate.height;
    let round = certificate.round;
    let value_id = certificate.value_id;
    info!(
        %height, %round, value = %certificate.value_id,
        "🟢🟢 Consensus has decided on value"
    );

    // The consensus engine only sends Decided messages for values (proposals)
    // that were completely received by the local node
    let block_bytes = state
        .get_block_data(height, round, value_id)
        .await
        .ok_or_eyre("app: certificate should have associated block data")?;
    debug!("🎁 block size: {:?}, height: {}", block_bytes.len(), height);

    // Decode bytes into execution payload (a block) and get relevant fields
    let execution_payload = ExecutionPayloadV3::from_ssz_bytes(&block_bytes).unwrap();
    let block_hash = execution_payload.payload_inner.payload_inner.block_hash;
    let block_timestamp = execution_payload.timestamp();
    let block_number = execution_payload.payload_inner.payload_inner.block_number;
    let block_prev_randao = execution_payload.payload_inner.payload_inner.prev_randao;
    let parent_block_hash = execution_payload.payload_inner.payload_inner.parent_hash;
    let tx_count = execution_payload
        .payload_inner
        .payload_inner
        .transactions
        .len();
    debug!("🦄 Block at height {height} contains {tx_count} transactions");

    // Sanity check: verify payload.parent_hash == state.latest_block.block_hash
    let latest_block_hash = state
        .latest_block
        .ok_or_eyre("missing latest block in state")?
        .block_hash;
    assert_eq!(latest_block_hash, parent_block_hash);

    // Log stats
    {
        state.txs_count += tx_count as u64;
        state.chain_bytes += block_bytes.len() as u64;
        let elapsed_time = state.start_time.elapsed();

        state.metrics.tx_stats.add_txs(tx_count as u64);
        state
            .metrics
            .tx_stats
            .add_chain_bytes(block_bytes.len() as u64);
        state
            .metrics
            .tx_stats
            .set_txs_per_second(state.txs_count as f64 / elapsed_time.as_secs_f64());
        state
            .metrics
            .tx_stats
            .set_bytes_per_second(state.chain_bytes as f64 / elapsed_time.as_secs_f64());
        state.metrics.tx_stats.set_block_tx_count(tx_count as u64);
        state
            .metrics
            .tx_stats
            .set_block_size(block_bytes.len() as u64);

        // Persist cumulative metrics to database for crash recovery
        state
            .store
            .store_cumulative_metrics(state.txs_count, state.chain_bytes, elapsed_time.as_secs())
            .await?;

        info!(
            "👉 stats at height {}: #txs={}, txs/s={:.2}, chain_bytes={}, bytes/s={:.2}",
            height,
            state.txs_count,
            state.txs_count as f64 / elapsed_time.as_secs_f64(),
            state.chain_bytes,
            state.chain_bytes as f64 / elapsed_time.as_secs_f64(),
        );
    }

    // Get validation status from cache or call newPayload
    let validity = if let Some(cached) = state.validated_cache_mut().get(&block_hash) {
        cached
    } else {
        // Collect hashes from blob transactions
        let block: Block = execution_payload.clone().try_into_block().map_err(|e| {
            eyre::eyre!(
                "Failed to convert decided ExecutionPayloadV3 to Block at height {}: {}",
                height,
                e
            )
        })?;
        let versioned_hashes: Vec<BlockHash> =
            block.body.blob_versioned_hashes_iter().copied().collect();

        // Ask the EL to validate the execution payload
        let payload_status = engine
            .notify_new_block(execution_payload, versioned_hashes)
            .await?;

        let validity = if payload_status.status.is_valid() {
            Validity::Valid
        } else {
            Validity::Invalid
        };

        // TODO: insert validation outcome into cache also when calling notify_new_block_with_retry in validate_payload
        state.validated_cache_mut().insert(block_hash, validity);
        validity
    };

    if validity == Validity::Invalid {
        return Err(eyre!("Block validation failed for hash: {}", block_hash));
    }

    debug!(
        "💡 Block validated at height {} with hash: {}",
        height, block_hash
    );

    // Notify the EL of the new block.
    // Update the execution head state to this block.
    let latest_valid_hash = engine
        .set_latest_forkchoice_state(block_hash, &emerald_config.retry_config)
        .await?;
    debug!(
        "🚀 Forkchoice updated to height {} for block hash={} and latest_valid_hash={}",
        height, block_hash, latest_valid_hash
    );

    // When that happens, we store the decided value in our store
    // TODO: we should return an error reply if commit fails
    state.commit(certificate).await?;

    // Save the latest block
    state.latest_block = Some(ExecutionBlock {
        block_hash: block_hash,
        block_number: block_number,
        parent_hash: latest_block_hash,
        timestamp: block_timestamp, // Note: This was a fix related to the sync reactor
        prev_randao: block_prev_randao,
    });

    // Update consensus_height and consensus_round to track the tip of the blockchain
    // After committing height H, the tip advances to H+1 where consensus will work next
    state.consensus_height = height.increment();
    state.consensus_round = Round::ZERO;

    // Get the new validator set for the next height and update the local state
    let new_validator_set =
        read_validators_from_contract(engine.eth.url().as_ref(), &latest_valid_hash).await?;
    debug!("🌈 Got validator set: {:?}", new_validator_set);
    state.set_validator_set(state.consensus_height, new_validator_set);

    // And then we instruct consensus to start the next height
    if reply
        .send(Next::Start(
            state.consensus_height,
            state
                .get_validator_set(state.consensus_height)
                .ok_or_eyre("Validator set not found for height {state.consensus_height}")?
                .clone(),
        ))
        .is_err()
    {
        error!("Failed to send Decided reply");
    }

    Ok(())
}

/// Handle ProcessSyncedValue messages from the consensus engine
///
/// Notifies the application that a value has been synced from the network.
/// This may happen when the node is catching up with the network.
///
/// If a value can be decoded from the bytes provided, then the application MUST reply
/// to this message with the decoded value. Otherwise, it MUST reply with `None`.
pub async fn on_process_synced_value(
    process_synced_value: AppMsg<EmeraldContext>,
    state: &mut State,
    engine: &Engine,
    emerald_config: &EmeraldConfig,
) -> eyre::Result<()> {
    let AppMsg::ProcessSyncedValue {
        height,
        round,
        proposer,
        value_bytes,
        reply,
    } = process_synced_value
    else {
        unreachable!("on_process_synced_value called with non-ProcessSyncedValue message");
    };

    info!(%height, %round, "🟢🟢 Processing synced value");

    let value = decode_value(value_bytes);

    // Extract execution payload from the synced value for validation
    let block_bytes = value.extensions.clone();
    let execution_payload = ExecutionPayloadV3::from_ssz_bytes(&block_bytes).map_err(|e| {
        eyre::eyre!(
            "Failed to decode synced ExecutionPayloadV3 at height {}: {:?}",
            height,
            e
        )
    })?;
    let new_block_hash = execution_payload.payload_inner.payload_inner.block_hash;

    // Collect hashes from blob transactions
    let block: Block = execution_payload.clone().try_into_block().map_err(|e| {
        eyre::eyre!(
            "Failed to convert synced ExecutionPayloadV3 to Block at height {}: {}",
            height,
            e
        )
    })?;
    let versioned_hashes: Vec<BlockHash> =
        block.body.blob_versioned_hashes_iter().copied().collect();

    // Validate the synced block
    let validity = validate_payload(
        state.validated_cache_mut(),
        engine,
        &execution_payload,
        &versioned_hashes,
        &emerald_config.retry_config,
        height,
        round,
    )
    .await?;

    if validity == Validity::Invalid {
        // Reject invalid blocks - don't store or reply with them
        if reply
            .send(Some(ProposedValue {
                height,
                round,
                valid_round: Round::Nil,
                proposer,
                value,
                validity: Validity::Invalid,
            }))
            .is_err()
        {
            error!("Failed to send ProcessSyncedValue rejection reply");
        }
        return Ok(());
    }

    debug!(
        "💡 Sync block validated at height {} with hash: {}",
        height, new_block_hash
    );
    let proposed_value: ProposedValue<EmeraldContext> = ProposedValue {
        height,
        round,
        valid_round: Round::Nil,
        proposer,
        value,
        validity: Validity::Valid,
    };

    if let Err(e) = state
        .store
        .store_undecided_block_data(height, round, proposed_value.value.id(), block_bytes)
        .await
    {
        error!(%height, %round, error = %e, "Failed to store synced block data");
    }
    // Store the synced value and block data
    if let Err(e) = state
        .store
        .store_undecided_proposal(proposed_value.clone())
        .await
    {
        error!(%height, %round, error = %e, "Failed to store synced value");
    }

    // Send to consensus to see if it has been decided on
    if reply.send(Some(proposed_value)).is_err() {
        error!(%height, %round, "Failed to send ProcessSyncedValue reply");
    }

    Ok(())
}

/// Handle GetDecidedValue messages from the consensus engine
///
/// Requests a previously decided value from the application's storage.
///
/// The application MUST respond with that value if available, or `None` otherwise.
pub async fn on_get_decided_value(
    get_decided_value: AppMsg<EmeraldContext>,
    state: &State,
    engine: &Engine,
) -> eyre::Result<()> {
    let AppMsg::GetDecidedValue { height, reply } = get_decided_value else {
        unreachable!("on_decided_value called with non-GetDecidedValue message");
    };

    info!(%height, "🟢🟢 GetDecidedValue");

    let earliest_height_available = state.get_earliest_height().await;
    // Check if requested height is beyond our consensus height
    let raw_decided_value = if (earliest_height_available..state.consensus_height).contains(&height)
    {
        let earliest_unpruned = state.get_earliest_unpruned_height().await;
        get_decided_value_for_sync(&state.store, engine, height, earliest_unpruned).await?
    } else {
        info!(%height, consensus_height = %state.consensus_height, "Requested height is >= consensus height or < earliest_height_available.");
        None
    };

    if reply.send(raw_decided_value).is_err() {
        error!("Failed to send GetDecidedValue reply");
    }

    Ok(())
}

/// Handle GetHistoryMinHeight messages from the consensus engine
///
/// Requests the earliest height available in the history maintained by the application.
///
/// The application MUST respond with its earliest available height.
pub async fn on_get_history_min_height(
    get_history_min_height: AppMsg<EmeraldContext>,
    state: &State,
) -> eyre::Result<()> {
    let AppMsg::GetHistoryMinHeight { reply } = get_history_min_height else {
        unreachable!("on_get_history_min_height called with non-GetHistoryMinHeight message");
    };

    let min_height = state.get_earliest_height().await;

    if reply.send(min_height).is_err() {
        error!("Failed to send GetHistoryMinHeight reply");
    }

    Ok(())
}

/// Handle RestreamProposal messages from the consensus engine
///
/// Requests the application to re-stream a proposal that it has already seen.
///
/// The application MUST re-publish again all the proposal parts pertaining
/// to that value by sending [`NetworkMsg::PublishProposalPart`] messages through
/// the [`Channels::network`] channel.
pub async fn on_restream_proposal(
    restream_proposal: AppMsg<EmeraldContext>,
    state: &mut State,
    channels: &mut Channels<EmeraldContext>,
) -> eyre::Result<()> {
    let AppMsg::RestreamProposal {
        height,
        round,
        valid_round,
        address,
        value_id,
    } = restream_proposal
    else {
        unreachable!("on_restream_proposal called with non-RestreamProposal message");
    };

    //  Look for a proposal at valid_round or round(should be already stored)
    let proposal_round = if valid_round == Round::Nil {
        round
    } else {
        valid_round
    };
    info!(%height, %proposal_round, "Restreaming existing proposal...");

    //let (proposal, bytes) =
    match state
        .get_previous_proposal_by_value_and_proposer(height, round, value_id, address)
        .await?
    {
        Some(proposal) => {
            info!(value = %proposal.value.id(), "Re-using previously built value");
            // Fetch the block data for the previously built value
            let bytes = state
                .store
                .get_block_data(height, round, proposal.value.id())
                .await?
                .ok_or_else(|| eyre!("Block data not found for previously built value"))?;
            // Now what's left to do is to break down the value to propose into parts,
            // and send those parts over the network to our peers, for them to re-assemble the full value.
            for stream_message in state.stream_proposal(proposal, bytes, proposal_round) {
                debug!(%height, %round, "Streaming proposal part: {stream_message:?}");
                channels
                    .network
                    .send(NetworkMsg::PublishProposalPart(stream_message))
                    .await?;
            }

            debug!(%height, %round, "✅ Re-sent proposal");
        }
        None => {
            debug!(%height, %round, "✅ No proposal to re-send");
        }
    }

    Ok(())
}

/// Handle ExtendVote messages from the consensus engine
///
/// ExtendVote allows the application to extend the pre-commit vote with arbitrary data.
///
/// When consensus is preparing to send a pre-commit vote, it first calls `ExtendVote`.
/// The application then returns a blob of data called a vote extension.
/// This data is opaque to the consensus algorithm but can contain application-specific information.
/// The proposer of the next block will receive all vote extensions along with the commit certificate.
pub async fn on_extended_vote(extended_vote: AppMsg<EmeraldContext>) -> eyre::Result<()> {
    let AppMsg::ExtendVote { reply, .. } = extended_vote else {
        unreachable!("on_extended_vote called with non-ExtendVote message");
    };

    if reply.send(None).is_err() {
        error!("🔴 Failed to send ExtendVote reply");
    }

    Ok(())
}

/// Handle VerifyVoteExtension messages from the consensus engine
///
/// Verify a vote extension
///
/// If the vote extension is deemed invalid, the vote it was part of
/// will be discarded altogether.
pub async fn on_verify_vote_extention(
    verify_vote_extenstion: AppMsg<EmeraldContext>,
) -> eyre::Result<()> {
    let AppMsg::VerifyVoteExtension { reply, .. } = verify_vote_extenstion else {
        unreachable!("on_verify_vote_extention called with non-VerifyVoteExtension message");
    };

    if reply.send(Ok(())).is_err() {
        error!("🔴 Failed to send VerifyVoteExtension reply");
    }

    Ok(())
}

pub async fn process_consensus_message(
    msg: AppMsg<EmeraldContext>,
    state: &mut State,
    channels: &mut Channels<EmeraldContext>,
    engine: &Engine,
    emerald_config: &EmeraldConfig,
) -> eyre::Result<()> {
    match msg {
        // The first message to handle is the `ConsensusReady` message, signaling to the app
        // that Malachite is ready to start consensus
        msg @ AppMsg::ConsensusReady { .. } => {
            on_consensus_ready(msg, state, engine, emerald_config).await?;
        }

        // The next message to handle is the `StartRound` message, signaling to the app
        // that consensus has entered a new round (including the initial round 0)
        msg @ AppMsg::StartedRound { .. } => {
            on_started_round(msg, state, engine, emerald_config).await?;
        }

        // At some point, we may end up being the proposer for that round, and the consensus engine
        // will then ask us for a value to propose to the other validators.
        msg @ AppMsg::GetValue { .. } => {
            on_get_value(msg, state, channels, engine, emerald_config).await?;
        }

        // On the receiving end of these proposal parts (ie. when we are not the proposer),
        // we need to process these parts and re-assemble the full value.
        // To this end, we store each part that we receive and assemble the full value once we
        // have all its constituent parts. Then we send that value back to consensus for it to
        // consider and vote for or against it (ie. vote `nil`), depending on its validity.
        msg @ AppMsg::ReceivedProposalPart { .. } => {
            on_received_proposal_part(msg, state, engine, emerald_config).await?;
        }

        // After some time, consensus will finally reach a decision on the value
        // to commit for the consensus_height, and will notify the application,
        // providing it with a commit certificate which contains the ID of the value
        // that was decided on as well as the set of commits for that value,
        // ie. the precommits together with their (aggregated) signatures.
        msg @ AppMsg::Decided { .. } => {
            on_decided(msg, state, engine, emerald_config).await?;
        }

        // It may happen that our node is lagging behind its peers. In that case,
        // a synchronization mechanism will automatically kick to try and catch up to
        // our peers. When that happens, some of these peers will send us decided values
        // for the heights in between the one we are currently at (included) and the one
        // that they are at. When the engine receives such a value, it will forward to the application
        // to decode it from its wire format and send back the decoded value to consensus.
        msg @ AppMsg::ProcessSyncedValue { .. } => {
            on_process_synced_value(msg, state, engine, emerald_config).await?;
        }

        // If, on the other hand, we are not lagging behind but are instead asked by one of
        // our peer to help them catch up because they are the one lagging behind,
        // then the engine might ask the application to provide with the value
        // that was decided at some lower height. In that case, we fetch it from our store
        // and send it to consensus.
        msg @ AppMsg::GetDecidedValue { .. } => {
            on_get_decided_value(msg, state, engine).await?;
        }

        // In order to figure out if we can help a peer that is lagging behind,
        // the engine may ask us for the height of the earliest available value in our store.
        msg @ AppMsg::GetHistoryMinHeight { .. } => {
            on_get_history_min_height(msg, state).await?;
        }

        msg @ AppMsg::RestreamProposal { .. } => {
            on_restream_proposal(msg, state, channels).await?;
        }

        msg @ AppMsg::ExtendVote { .. } => {
            on_extended_vote(msg).await?;
        }

        msg @ AppMsg::VerifyVoteExtension { .. } => {
            on_verify_vote_extention(msg).await?;
        }
    }

    Ok(())
}

pub async fn run(
    state: &mut State,
    channels: &mut Channels<EmeraldContext>,
    engine: Engine,
    emerald_config: EmeraldConfig,
) -> eyre::Result<()> {
    while let Some(msg) = channels.consensus.recv().await {
        process_consensus_message(msg, state, channels, &engine, &emerald_config).await?;
    }

    // If we get there, it can only be because the channel we use to receive message
    // from consensus has been closed, meaning that the consensus actor has died.
    // We can do nothing but return an error here.
    Err(eyre!("Consensus channel closed unexpectedly"))
}
