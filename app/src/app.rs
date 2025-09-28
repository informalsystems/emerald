use alloy_provider::ProviderBuilder;
use bytes::Bytes;
use color_eyre::eyre::{self, eyre};
use ed25519_consensus::VerificationKey;
use ssz::{Decode, Encode};
use tracing::{debug, error, info};

use alloy_rpc_types_engine::ExecutionPayloadV3;
use malachitebft_app_channel::app::streaming::StreamContent;
use malachitebft_app_channel::app::types::codec::Codec;
use malachitebft_app_channel::app::types::core::{Round, Validity};
use malachitebft_app_channel::app::types::sync::RawDecidedValue;
use malachitebft_app_channel::app::types::{LocallyProposedValue, ProposedValue};
use malachitebft_app_channel::{AppMsg, Channels, ConsensusMsg, NetworkMsg};

use alloy_primitives::{address, Address};
use malachitebft_eth_engine::engine::Engine;
use malachitebft_eth_engine::json_structures::ExecutionBlock;
use malachitebft_eth_types::codec::proto::ProtobufCodec;
use malachitebft_eth_types::{
    Block, BlockHash, PublicKey, TestContext, ValidatorSet as ValidatorSetType,
};

const GENESIS_VALIDATOR_SET_ACCOUNT: Address = address!("0000000000000000000000000000000000002000");

use crate::state::{decode_value, State};

pub async fn run(
    state: &mut State,
    channels: &mut Channels<TestContext>,
    engine: Engine,
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

                // Get the latest block from the execution engine
                let latest_block = engine.eth.get_block_by_number("latest").await?.unwrap();
                debug!("👉 latest_block: {:?}", latest_block);
                state.latest_block = Some(latest_block);

                // We can simply respond by telling the engine to start consensus
                // at the current height, which is initially 1
                if reply
                    .send(ConsensusMsg::StartHeight(
                        state.current_height,
                        state.get_validator_set().clone(),
                    ))
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
            } => {
                info!(%height, %round, %proposer, "🟢🟢 Started round");

                // We can use that opportunity to update our internal state
                state.current_height = height;
                state.current_round = round;
                state.current_proposer = Some(proposer);
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
                let execution_payload = engine.generate_block(&latest_block).await?;
                debug!("🌈 Got execution payload: {:?}", execution_payload);

                // Store block in state and propagate to peers.
                let bytes = Bytes::from(execution_payload.as_ssz_bytes());
                debug!("🎁 block size: {:?}, height: {}", bytes.len(), height);

                // Prepare block proposal.
                let proposal: LocallyProposedValue<TestContext> =
                    state.propose_value(height, round, bytes.clone()).await?;

                // When the node is not the proposer, store the block data,
                // which will be passed to the execution client (EL) on commit.
                state.store_undecided_proposal_data(bytes.clone()).await?;

                // Send it to consensus
                if reply.send(proposal.clone()).is_err() {
                    error!("Failed to send GetValue reply");
                }

                // Now what's left to do is to break down the value to propose into parts,
                // and send those parts over the network to our peers, for them to re-assemble the full value.
                for stream_message in state.stream_proposal(proposal, bytes) {
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

            // In some cases, e.g. to verify the signature of a vote received at a higher height
            // than the one we are at (e.g. because we are lagging behind a little bit),
            // the engine may ask us for the validator set at that height.
            //
            // In our case, our validator set stays constant between heights so we can
            // send back the validator set found in our genesis state.
            AppMsg::GetValidatorSet { height: _, reply } => {
                if reply.send(state.get_validator_set().clone()).is_err() {
                    error!("🔴 Failed to send GetValidatorSet reply");
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
                    .expect("certificate should have associated block data");
                debug!("🎁 block size: {:?}, height: {}", block_bytes.len(), height);

                // Decode bytes into execution payload (a block)
                let execution_payload = ExecutionPayloadV3::from_ssz_bytes(&block_bytes).unwrap();

                let parent_block_hash = execution_payload.payload_inner.payload_inner.parent_hash;

                let new_block_hash = execution_payload.payload_inner.payload_inner.block_hash;

                assert_eq!(state.latest_block.unwrap().block_hash, parent_block_hash);

                let block_number = execution_payload.payload_inner.payload_inner.block_number;

                alloy_sol_types::sol!(
                    #[sol(rpc)]
                    ValidatorSet,
                    "../solidity/out/ValidatorSet.sol/ValidatorSet.json"
                );

                // call the ValidatorSet contract to get the list of validators

                let provider = ProviderBuilder::new()
                    .on_builtin(&engine.eth.url().to_string())
                    .await?;
                let validator_set_contract =
                    ValidatorSet::new(GENESIS_VALIDATOR_SET_ACCOUNT, provider);
                let new_validator_set_sol = validator_set_contract
                    .getValidators()
                    .block(block_number.into())
                    .call()
                    .await?;

                let new_validator_set = ValidatorSetType::new(
                    new_validator_set_sol
                        .validators
                        .into_iter()
                        .map(
                            |ValidatorSet::ValidatorInfoFull {
                                 ed25519Key, power, ..
                             }| {
                                let pub_key_bytes: [u8; 32] = ed25519Key.into();
                                let pub_key = PublicKey::new(
                                    VerificationKey::try_from(pub_key_bytes).unwrap(),
                                );
                                malachitebft_eth_types::Validator::new(
                                    pub_key,
                                    power.try_into().unwrap(),
                                )
                            },
                        )
                        .collect::<Vec<_>>(),
                );

                debug!("🌈 Got validator set: {:?}", new_validator_set);

                state.validator_set = new_validator_set;

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
                info!(
                    "👉 stats at height {}: #txs={}, txs/s={:.2}, chain_bytes={}, bytes/s={:.2}",
                    height,
                    state.txs_count,
                    state.txs_count as f64 / elapsed_time.as_secs_f64(),
                    state.chain_bytes,
                    state.chain_bytes as f64 / elapsed_time.as_secs_f64(),
                );

                let tx_count = execution_payload
                    .payload_inner
                    .payload_inner
                    .transactions
                    .len();
                debug!("🦄 Block at height {height} contains {tx_count} transactions");

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
                state.commit(certificate).await?;

                // Save the latest block
                state.latest_block = Some(ExecutionBlock {
                    block_hash: new_block_hash,
                    block_number: new_block_number,
                    parent_hash: latest_valid_hash,
                    timestamp: new_block_timestamp,
                    prev_randao: new_block_prev_randao,
                });

                // Pause briefly before starting next height, just to make following the logs easier
                // tokio::time::sleep(Duration::from_millis(500)).await;

                // And then we instruct consensus to start the next height
                if reply
                    .send(ConsensusMsg::StartHeight(
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
            //
            // TODO: store the received value somewhere here
            AppMsg::ProcessSyncedValue {
                height,
                round,
                proposer,
                value_bytes,
                reply,
            } => {
                info!(%height, %round, "🟢🟢 Processing synced value");

                let value = decode_value(value_bytes);

                // We send to consensus to see if it has been decided on
                if reply
                    .send(ProposedValue {
                        height,
                        round,
                        valid_round: Round::Nil,
                        proposer,
                        value,
                        validity: Validity::Valid,
                    })
                    .is_err()
                {
                    error!("Failed to send ProcessSyncedValue reply");
                }
            }

            // If, on the other hand, we are not lagging behind but are instead asked by one of
            // our peer to help them catch up because they are the one lagging behind,
            // then the engine might ask the application to provide with the value
            // that was decided at some lower height. In that case, we fetch it from our store
            // and send it to consensus.
            AppMsg::GetDecidedValue { height, reply } => {
                info!(%height, "🟢🟢 GetDecidedValue");
                let decided_value = state.get_decided_value(height).await;

                let raw_decided_value = decided_value.map(|decided_value| RawDecidedValue {
                    certificate: decided_value.certificate,
                    value_bytes: ProtobufCodec.encode(&decided_value.value).unwrap(),
                });

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

            AppMsg::PeerJoined { peer_id } => {
                info!(%peer_id, "🟢🟢 Peer joined our local view of network");

                // You might want to track connected peers in your state
                state.peers.insert(peer_id);
            }

            AppMsg::PeerLeft { peer_id } => {
                info!(%peer_id, "🔴 Peer left our local view of network");

                // Remove the peer from tracking
                state.peers.remove(&peer_id);
            }
        }
    }

    // If we get there, it can only be because the channel we use to receive message
    // from consensus has been closed, meaning that the consensus actor has died.
    // We can do nothing but return an error here.
    Err(eyre!("Consensus channel closed unexpectedly"))
}
