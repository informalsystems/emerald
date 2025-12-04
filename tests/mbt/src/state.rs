use itf::de::{self, As};
use std::collections::{BTreeMap, BTreeSet};

use quint_connect::{Result, State as QuintState};
use serde::Deserialize;

use crate::driver::EmeraldDriver;

/// App operational phase (from spec line 65-68)
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize)]
#[serde(tag = "tag")]
pub enum AppPhase {
    Uninitialized,
    Ready,
    Operating,
}

/// Sync status for catching up (from spec line 71-74)
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize)]
#[serde(tag = "tag")]
pub enum SyncStatus {
    NotSyncing,
    CatchingUp,
    Providing,
}

/// Validity status for proposals (from spec line 56)
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(tag = "tag")]
pub enum Validity {
    Valid,
    Invalid,
}

/// Proposal storage states (from spec line 58-62)
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(tag = "tag")]
pub enum ProposalStatus {
    Pending,
    Undecided,
    DecidedStatus,
}

/// Proposal record (from spec line 77-85)
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, Hash, Deserialize)]
pub struct Proposal {
    pub height: i64,
    pub round: i64,
    pub proposer: String,
    pub value_id: (i64, i64),
    pub payload: String,
    pub validity: Validity,
    pub status: ProposalStatus,
}

type Address = String;

/// Individual node state (mirrors spec AppState)
#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct SpecState {
    pub process_id: String,
    pub phase: AppPhase,

    // Current consensus state
    pub current_height: i64,
    pub current_round: i64,
    #[serde(with = "As::<de::Option::<_>>")]
    pub current_proposer: Option<Address>,

    // // Latest committed block
    pub latest_block_height: i64,
    #[serde(with = "As::<de::Option::<_>>")]
    pub latest_block_hash: Option<String>,

    // // Validator set
    pub validator_set: BTreeSet<String>,

    // // Proposals in various states
    pub proposals: BTreeSet<Proposal>,

    // Sync state
    pub sync_status: SyncStatus,
    pub min_history_height: i64,
}

/// State extracted from the Emerald app (system map)
#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct EmeraldState(pub BTreeMap<String, SpecState>);

impl QuintState<EmeraldDriver> for EmeraldState {
    fn from_driver(driver: &EmeraldDriver) -> Result<Self> {
        use malachitebft_app_channel::app::types::core::Validity as MalachiteValidity;

        let mut system_map = BTreeMap::new();

        for (node_id, components) in &driver.nodes {
            let state = &components.state;

            // Extract basic consensus state
            let current_height = state.current_height.as_u64() as i64;
            let current_round = state.current_round.as_u32().unwrap_or(0) as i64;
            // Map proposer address to node name using the driver's address mapping
            let current_proposer = state.current_proposer.as_ref()
                .and_then(|addr| driver.address_to_node.get(addr).cloned());

            // Extract latest block info
            let (latest_block_height, latest_block_hash) = if let Some(block) = &state.latest_block {
                // Abstract block hashes to match the spec's abstraction:
                // - Genesis block (height 0) -> "genesis"
                // - All other blocks -> "block"
                let hash = if block.block_number == 0 {
                    "genesis".to_string()
                } else {
                    "block".to_string()  // Abstract all non-genesis blocks to "block"
                };
                (block.block_number as i64, Some(hash))
            } else {
                (0, None)
            };

            // Extract validator set
            // Use the driver's address mappings to convert addresses to node names
            let validator_set = if let Some(vs) = state.try_get_validator_set() {
                vs.validators
                    .iter()
                    .filter_map(|v| {
                        // Look up node name from address
                        driver.address_to_node.get(&v.address).cloned()
                    })
                    .collect()
            } else {
                BTreeSet::new() // Empty validator set if not initialized
            };

            // Collect proposals from pending and undecided
            // Note: We need to use block_on because Store methods are async
            let proposals = driver.runtime.block_on(async {
                let mut all_proposals = BTreeSet::new();

                // Get pending proposals (status = Pending)
                let pending_result = state.store
                    .get_pending_proposal_parts(state.current_height, state.current_round)
                    .await;

                if let Ok(pending_parts) = pending_result {
                    for parts in pending_parts {
                        // Extract value_id from Init part if available
                        let value_id = if let Some(init) = parts.init() {
                            (init.height.as_u64() as i64, init.round.as_u32().unwrap_or(0) as i64)
                        } else {
                            (parts.height.as_u64() as i64, parts.round.as_u32().unwrap_or(0) as i64)
                        };

                        // Map proposer address to node name
                        let proposer = driver.address_to_node
                            .get(&parts.proposer)
                            .cloned()
                            .unwrap_or_else(|| format!("{:?}", parts.proposer));

                        let proposal = Proposal {
                            height: parts.height.as_u64() as i64,
                            round: parts.round.as_u32().unwrap_or(0) as i64,
                            proposer,
                            value_id,
                            payload: String::new(), // Pending proposals don't have full payload yet
                            validity: Validity::Valid, // Assume valid until proven otherwise
                            status: ProposalStatus::Pending,
                        };
                        all_proposals.insert(proposal);
                    }
                }

                // Get undecided proposals (status = Undecided)
                let undecided_result = state.store
                    .get_undecided_proposals(state.current_height, state.current_round)
                    .await;

                if let Ok(undecided) = undecided_result {
                    for prop_value in undecided {
                        // In the spec, value_id is (Height, Round) not the actual value hash
                        let value_id = (
                            prop_value.height.as_u64() as i64,
                            prop_value.round.as_u32().unwrap_or(0) as i64
                        );

                        // Abstract the payload to match the spec
                        // The value.extensions contains SSZ-encoded ExecutionPayload (binary data)
                        // For the spec, we abstract this to just "payload"
                        let payload = "payload".to_string();

                        // Map proposer address to node name
                        let proposer = driver.address_to_node
                            .get(&prop_value.proposer)
                            .cloned()
                            .unwrap_or_else(|| format!("{:?}", prop_value.proposer));

                        let proposal = Proposal {
                            height: prop_value.height.as_u64() as i64,
                            round: prop_value.round.as_u32().unwrap_or(0) as i64,
                            proposer,
                            value_id,
                            payload,
                            validity: match prop_value.validity {
                                MalachiteValidity::Valid => Validity::Valid,
                                MalachiteValidity::Invalid => Validity::Invalid,
                            },
                            status: ProposalStatus::Undecided,
                        };
                        all_proposals.insert(proposal);
                    }
                }

                // Note: Decided proposals are stored separately via get_decided_value
                // and would have status = DecidedStatus
                // For now, we focus on pending and undecided proposals

                all_proposals
            });

            // Determine the phase based on the state
            // - Uninitialized: Before ConsensusReady (height=0, round=0, no proposals, empty validator set)
            // - Ready: After ConsensusReady but before StartedRound
            // - Operating: Actively processing consensus (height > 0 OR round > 0 OR has proposals)
            let phase = if current_height == 0 && current_round == 0
                && proposals.is_empty() && validator_set.is_empty() {
                AppPhase::Uninitialized
            } else if current_height > 0 || current_round > 0 || !proposals.is_empty() {
                AppPhase::Operating
            } else {
                AppPhase::Ready
            };

            let spec_state = SpecState {
                process_id: node_id.clone(),
                phase,
                current_height,
                current_round,
                current_proposer,
                latest_block_height,
                latest_block_hash,
                validator_set,
                proposals,
                sync_status: SyncStatus::NotSyncing, // Default to not syncing
                min_history_height: 0, // Default to 0 (could be derived from pruning settings)
            };

            system_map.insert(node_id.clone(), spec_state);
        }

        Ok(EmeraldState(system_map))
    }
}
