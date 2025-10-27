use std::time::Duration;

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
use malachitebft_eth_cli::config::MalakethConfig;
use malachitebft_eth_engine::engine::Engine;
use malachitebft_eth_engine::json_structures::ExecutionBlock;
use malachitebft_eth_types::secp256k1::PublicKey;
use malachitebft_eth_types::{Block, BlockHash, Height, MalakethContext, Validator, ValidatorSet};
use ssz::{Decode, Encode};
use tracing::{debug, error, info};

const GENESIS_VALIDATOR_MANAGER_ACCOUNT: Address =
    address!("0x0000000000000000000000000000000000002000");

alloy_sol_types::sol!(
    #[derive(Debug)]
    #[sol(rpc)]
    ValidatorManager,
    "../solidity/out/ValidatorManager.sol/ValidatorManager.json"
);

use crate::state::{decode_value, extract_block_header, State};
use crate::sync_handler::{get_decided_value_for_sync, validate_synced_payload};

pub async fn initialize_state_from_genesis(state: &mut State, engine: &Engine) -> eyre::Result<()> {
    // TODO Unify this with code above @Jasmina
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
    state.set_validator_set(genesis_validator_set);
    state.current_height = Height::default();
    Ok(())
}

pub async fn initialize_state_from_existing_block(
    state: &mut State,
    engine: &Engine,
    start_height: Height,
) -> eyre::Result<()> {
    // If there was somethign stored in the store for height, we should be able to retrieve
    // block data as well.

    let latest_block_candidate_from_store = state
        .get_latest_block_candidate(start_height)
        .await
        .ok_or_eyre("we have not atomically stored the last block, database corrupted")?;

    let payload_status = engine
        .send_forkchoice_updated(latest_block_candidate_from_store.block_hash)
        .await?;
    match payload_status.status {
        PayloadStatusEnum::Valid => {
            state.current_height = start_height;
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
            info!("latest block {:?}", state.latest_block);
            let block_validator_set = read_validators_from_contract(
                engine.eth.url().as_ref(),
                &latest_block_candidate_from_store.block_hash,
            )
            .await?;
            debug!("🌈 Got block validator set: {:?}", block_validator_set);
            state.set_validator_set(block_validator_set);
            Ok(())
        }
        PayloadStatusEnum::Invalid { validation_error } => Err(eyre::eyre!(validation_error)),

        PayloadStatusEnum::Accepted => Err(eyre::eyre!(
            "execution engine returned ACCEPTED for payload, this should not happen"
        )),
    }
}

pub async fn read_validators_from_contract(
    eth_url: &str,
    block_hash: &BlockHash,
) -> eyre::Result<ValidatorSet> {
    let provider = ProviderBuilder::new().on_builtin(eth_url).await?;

    let validator_manager_contract =
        ValidatorManager::new(GENESIS_VALIDATOR_MANAGER_ACCOUNT, provider);

    let genesis_validator_set_sol = validator_manager_contract
        .getValidators()
        .block((*block_hash).into())
        .call()
        .await?;

    let validators = genesis_validator_set_sol
        .validators
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

pub async fn run(
    state: &mut State,
    channels: &mut Channels<MalakethContext>,
    engine: Engine,
    malaketh_config: MalakethConfig,
) -> eyre::Result<()> {
    while let Some(msg) = channels.consensus.recv().await {
        match msg {
            // The first message to handle is the `ConsensusReady` message, signaling to the app
            // that Malachite is ready to start consensus
            AppMsg::ConsensusReady { reply } => {
                info!("🟢🟢 Consensus is ready");

                // Node start-up: https://hackmd.io/@danielrachi/engine_api#Node-startup
                // Check compatibility with execution client
                engine.check_capabilities().await?;

                // Get latest state from local store
                let start_height_from_store = state.store.max_decided_value_height().await;

                match start_height_from_store {
                    Some(s) => {
                        initialize_state_from_existing_block(state, &engine, s).await?;
                        info!(
                            "Start height in state set to: {:?}; height in store is {s} ",
                            state.current_height
                        );
                    }
                    None => {
                        info!("Starting from genesis");
                        // Get the genesis block from the execution engine
                        initialize_state_from_genesis(state, &engine).await?;
                    }
                }
                let start_height = state.current_height.increment();

                // We can simply respond by telling the engine to start consensus
                // at the current height, which is initially 1
                if reply
                    .send((start_height, state.get_validator_set().clone()))
                    .is_err()
                {
                    error!("Failed to send ConsensusReady reply");
                }
            }

            // The next message to handle is the `StartRound` message, signaling to the app
            // that consensus has entered a new round (including the initial round 0)
            AppMsg::StartedRound {
                height,
                round,
                proposer,
                role,
                reply_value,
            } => {
                info!(%height, %round, %proposer, ?role, "🟢🟢 Started round");

                // We can use that opportunity to update our internal state
                state.current_height = height;
                state.current_round = round;
                state.current_proposer = Some(proposer);

                // TODO: Add pending parts validation
                // Try to get undecided proposals from storage, or use empty vector
                let proposals: Vec<ProposedValue<MalakethContext>> = match state
                    .store
                    .get_undecided_proposal(height, round)
                    .await
                {
                    Ok(Some(proposal)) => {
                        vec![proposal]
                    }
                    Ok(None) => {
                        info!(%height, %round, "📭 Found 0 undecided proposals");
                        Vec::new()
                    }
                    Err(e) => {
                        error!(%height, %round, error = %e, "Failed to get undecided proposal from store");
                        return Err(e.into());
                    }
                };

                if !proposals.is_empty() {
                    info!(%height, %round, "📬 Found {} undecided proposals", proposals.len());
                }

                if reply_value.send(proposals).is_err() {
                    error!("Failed to send undecided proposals");
                }
            }

            // At some point, we may end up being the proposer for that round, and the consensus engine
            // will then ask us for a value to propose to the other validators.
            AppMsg::GetValue {
                height,
                round,
                timeout: _,
                reply,
            } => {
                // NOTE: We can ignore the timeout as we are building the value right away.
                // If we were let's say reaping as many txes from a mempool and executing them,
                // then we would need to respect the timeout and stop at a certain point.

                info!(%height, %round, "🟢🟢 Consensus is requesting a value to propose");

                // We need to ask the execution engine for a new value to
                // propose. Then we send it back to consensus.

                let latest_block = state.latest_block.expect("Head block hash is not set");
                let execution_payload = engine.generate_block(&Some(latest_block)).await?;

                debug!("🌈 Got execution payload: {:?}", execution_payload);

                // Store block in state and propagate to peers.
                let bytes = Bytes::from(execution_payload.as_ssz_bytes());
                debug!("🎁 block size: {:?}, height: {}", bytes.len(), height);

                // Prepare block proposal.
                let proposal: LocallyProposedValue<MalakethContext> =
                    state.propose_value(height, round, bytes.clone()).await?;

                // Store the block data at the proposal's height/round,
                // which will be passed to the execution client (EL) on commit.
                state
                    .store_undecided_proposal_data(height, round, bytes.clone())
                    .await?;

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
                    info!(%height, %round, "Streaming proposal part: {stream_message:?}");
                    channels
                        .network
                        .send(NetworkMsg::PublishProposalPart(stream_message))
                        .await?;
                }
                debug!(%height, %round, "✅ Proposal sent");
            }

            // On the receiving end of these proposal parts (ie. when we are not the proposer),
            // we need to process these parts and re-assemble the full value.
            // To this end, we store each part that we receive and assemble the full value once we
            // have all its constituent parts. Then we send that value back to consensus for it to
            // consider and vote for or against it (ie. vote `nil`), depending on its validity.
            AppMsg::ReceivedProposalPart { from, part, reply } => {
                let (part_type, part_size) = match &part.content {
                    StreamContent::Data(part) => (part.get_type(), part.size_bytes()),
                    StreamContent::Fin => ("end of stream", 0),
                };

                info!(
                    %from, %part.sequence, part.type = %part_type, part.size = %part_size,
                    "Received proposal part"
                );

                let proposed_value = state.received_proposal_part(from, part).await?;
                if let Some(proposed_value) = proposed_value.clone() {
                    debug!("✅ Received complete proposal: {:?}", proposed_value);
                }

                if reply.send(proposed_value).is_err() {
                    error!("Failed to send ReceivedProposalPart reply");
                }
            }

            // After some time, consensus will finally reach a decision on the value
            // to commit for the current height, and will notify the application,
            // providing it with a commit certificate which contains the ID of the value
            // that was decided on as well as the set of commits for that value,
            // ie. the precommits together with their (aggregated) signatures.
            AppMsg::Decided {
                certificate, reply, ..
            } => {
                let height = certificate.height;
                let round = certificate.round;
                info!(
                    %height, %round, value = %certificate.value_id,
                    "🟢🟢 Consensus has decided on value"
                );

                let block_bytes = state
                    .get_block_data(height, round)
                    .await
                    .ok_or_eyre("app: certificate should have associated block data")?;
                debug!("🎁 block size: {:?}, height: {}", block_bytes.len(), height);

                // Decode bytes into execution payload (a block)
                let execution_payload = ExecutionPayloadV3::from_ssz_bytes(&block_bytes).unwrap();

                let parent_block_hash = execution_payload.payload_inner.payload_inner.parent_hash;

                let new_block_hash = execution_payload.payload_inner.payload_inner.block_hash;

                assert_eq!(state.latest_block.unwrap().block_hash, parent_block_hash);

                let new_block_timestamp = execution_payload.timestamp();
                let new_block_number = execution_payload.payload_inner.payload_inner.block_number;

                let new_block_prev_randao =
                    execution_payload.payload_inner.payload_inner.prev_randao;

                // Log stats
                let tx_count = execution_payload
                    .payload_inner
                    .payload_inner
                    .transactions
                    .len();
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
                    .store_cumulative_metrics(
                        state.txs_count,
                        state.chain_bytes,
                        elapsed_time.as_secs(),
                    )
                    .await?;

                info!(
                    "👉 stats at height {}: #txs={}, txs/s={:.2}, chain_bytes={}, bytes/s={:.2}",
                    height,
                    state.txs_count,
                    state.txs_count as f64 / elapsed_time.as_secs_f64(),
                    state.chain_bytes,
                    state.chain_bytes as f64 / elapsed_time.as_secs_f64(),
                );

                let tx_count: usize = execution_payload
                    .payload_inner
                    .payload_inner
                    .transactions
                    .len();
                debug!("🦄 Block at height {height} contains {tx_count} transactions");

                // Extract block header
                let block_header = extract_block_header(&execution_payload);
                let block_header_bytes = Bytes::from(block_header.as_ssz_bytes());

                // Collect hashes from blob transactions
                let block: Block = execution_payload.clone().try_into_block().unwrap();
                let versioned_hashes: Vec<BlockHash> =
                    block.body.blob_versioned_hashes_iter().copied().collect();

                let payload_status = engine
                    .notify_new_block(execution_payload, versioned_hashes)
                    .await?;
                if payload_status.status.is_invalid() {
                    return Err(eyre!("Invalid payload status: {}", payload_status.status));
                }
                debug!(
                    "💡 New block added at height {} with hash: {}",
                    height, new_block_hash
                );

                // Notify the execution client (EL) of the new block.
                // Update the execution head state to this block.
                let latest_valid_hash = engine.set_latest_forkchoice_state(new_block_hash).await?;
                debug!(
                    "🚀 Forkchoice updated to height {} for block hash={} and latest_valid_hash={}",
                    height, new_block_hash, latest_valid_hash
                );

                // When that happens, we store the decided value in our store
                // TODO: we should return an error reply if commit fails
                state.commit(certificate, block_header_bytes).await?;
                let old_hash = state
                    .latest_block
                    .ok_or_eyre("missing latest block in state")?
                    .block_hash;
                // Save the latest block
                state.latest_block = Some(ExecutionBlock {
                    block_hash: new_block_hash,
                    block_number: new_block_number,
                    parent_hash: old_hash,
                    timestamp: new_block_timestamp, // Note: This was a fix related to the sync reactor
                    prev_randao: new_block_prev_randao,
                });

                let new_validator_set =
                    read_validators_from_contract(engine.eth.url().as_ref(), &latest_valid_hash)
                        .await?;
                debug!("🌈 Got validator set: {:?}", new_validator_set);
                state.set_validator_set(new_validator_set);

                // Pause briefly before starting next height, just to make following the logs easier
                // tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                // And then we instruct consensus to start the next height
                if reply
                    .send(Next::Start(
                        state.current_height,
                        state.get_validator_set().clone(),
                    ))
                    .is_err()
                {
                    error!("Failed to send Decided reply");
                }
            }

            // It may happen that our node is lagging behind its peers. In that case,
            // a synchronization mechanism will automatically kick to try and catch up to
            // our peers. When that happens, some of these peers will send us decided values
            // for the heights in between the one we are currently at (included) and the one
            // that they are at. When the engine receives such a value, it will forward to the application
            // to decode it from its wire format and send back the decoded value to consensus.
            AppMsg::ProcessSyncedValue {
                height,
                round,
                proposer,
                value_bytes,
                reply,
            } => {
                info!(%height, %round, "🟢🟢 Processing synced value");

                let value = decode_value(value_bytes);

                // Extract execution payload from the synced value for validation
                let block_bytes = value.extensions.clone();
                let execution_payload = ExecutionPayloadV3::from_ssz_bytes(&block_bytes).unwrap();
                let new_block_hash = execution_payload.payload_inner.payload_inner.block_hash;

                // Collect hashes from blob transactions
                let block: Block = execution_payload.clone().try_into_block().unwrap();
                let versioned_hashes: Vec<BlockHash> =
                    block.body.blob_versioned_hashes_iter().copied().collect();

                // Validate the synced block
                let validity = validate_synced_payload(
                    &engine,
                    &execution_payload,
                    &versioned_hashes,
                    Duration::from_millis(malaketh_config.sync_timeout_ms),
                    Duration::from_millis(malaketh_config.sync_initial_delay_ms),
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
                    continue;
                }

                debug!(
                    "💡 Sync block validated at height {} with hash: {}",
                    height, new_block_hash
                );
                let proposed_value = ProposedValue {
                    height,
                    round,
                    valid_round: Round::Nil,
                    proposer,
                    value,
                    validity: Validity::Valid,
                };

                // Store the synced value and block data
                if let Err(e) = state
                    .store
                    .store_undecided_proposal(proposed_value.clone())
                    .await
                {
                    error!(%height, %round, error = %e, "Failed to store synced value");
                }

                if let Err(e) = state
                    .store
                    .store_undecided_block_data(height, round, block_bytes)
                    .await
                {
                    error!(%height, %round, error = %e, "Failed to store synced block data");
                }

                // Send to consensus to see if it has been decided on
                if reply.send(Some(proposed_value)).is_err() {
                    error!(%height, %round, "Failed to send ProcessSyncedValue reply");
                }
            }

            // If, on the other hand, we are not lagging behind but are instead asked by one of
            // our peer to help them catch up because they are the one lagging behind,
            // then the engine might ask the application to provide with the value
            // that was decided at some lower height. In that case, we fetch it from our store
            // and send it to consensus.
            AppMsg::GetDecidedValue { height, reply } => {
                info!(%height, "🟢🟢 GetDecidedValue");

                let earliest_height_available = state.get_earliest_height().await;
                // Check if requested height is beyond our current height
                let raw_decided_value = if (earliest_height_available..state.current_height)
                    .contains(&height)
                {
                    let earliest_unpruned = state.get_earliest_unpruned_height().await;
                    get_decided_value_for_sync(&state.store, &engine, height, earliest_unpruned)
                        .await?
                } else {
                    info!(%height, current_height = %state.current_height, "Requested height is >= current height or < earliest_height_available.");
                    None
                };

                if reply.send(raw_decided_value).is_err() {
                    error!("Failed to send GetDecidedValue reply");
                }
            }

            // In order to figure out if we can help a peer that is lagging behind,
            // the engine may ask us for the height of the earliest available value in our store.
            AppMsg::GetHistoryMinHeight { reply } => {
                let min_height = state.get_earliest_height().await;

                if reply.send(min_height).is_err() {
                    error!("Failed to send GetHistoryMinHeight reply");
                }
            }

            AppMsg::RestreamProposal { .. } => {
                error!("🔴 RestreamProposal not implemented");
            }

            AppMsg::ExtendVote { reply, .. } => {
                if reply.send(None).is_err() {
                    error!("🔴 Failed to send ExtendVote reply");
                }
            }

            AppMsg::VerifyVoteExtension { reply, .. } => {
                if reply.send(Ok(())).is_err() {
                    error!("🔴 Failed to send VerifyVoteExtension reply");
                }
            }
        }
    }

    // If we get there, it can only be because the channel we use to receive message
    // from consensus has been closed, meaning that the consensus actor has died.
    // We can do nothing but return an error here.
    Err(eyre!("Consensus channel closed unexpectedly"))
}
