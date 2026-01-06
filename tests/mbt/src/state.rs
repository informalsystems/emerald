use std::collections::{BTreeMap, BTreeSet};

use emerald::state::assemble_value_from_parts;
use emerald::State;
use itf::de::{self, As};
use malachitebft_eth_types::Height as EmeraldHeight;
use quint_connect::{Result, State as QuintState};
use serde::Deserialize;

use crate::driver::EmeraldDriver;

pub type Node = String;
pub type Height = u64;
pub type Round = u32;
pub type Payload = u64;
pub type ValueId = (Height, Round, Payload);

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, Hash, Deserialize)]
pub struct Proposal {
    pub height: Height,
    pub round: Round,
    pub proposer: Node,
    pub payload: Payload,
}

impl Proposal {
    pub fn id(&self) -> ValueId {
        (self.height, self.round, self.payload)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct NodeState {
    pub current_height: Height,
    pub current_round: Round,
    pub latest_block_height: Height,
    #[serde(with = "As::<de::Option::<_>>")]
    pub latest_block_payload: Option<Payload>,
    pub proposals: BTreeSet<Proposal>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct SpecState(pub BTreeMap<Node, NodeState>);

impl QuintState<EmeraldDriver> for SpecState {
    fn from_driver(driver: &EmeraldDriver) -> Result<Self> {
        let mut system = BTreeMap::new();

        for (node, components) in &driver.nodes {
            let state = &components.state;
            let proposals = driver.runtime.block_on(get_proposals(driver, state));
            let (latest_block_height, latest_block_payload) = get_latest_block_info(driver, state);

            let spec_state = NodeState {
                current_height: state.current_height.as_u64(),
                current_round: state.current_round.as_u32().unwrap_or(0),
                latest_block_height,
                latest_block_payload,
                proposals,
            };

            system.insert(node.clone(), spec_state);
        }

        Ok(SpecState(system))
    }
}

fn get_latest_block_info(driver: &EmeraldDriver, state: &State) -> (u64, Option<u64>) {
    let (latest_block_height, latest_block_payload) = match &state.latest_block {
        None => (0, None),                                   // genesis
        Some(block) if block.block_number == 0 => (0, None), // genesis
        Some(block) => {
            let payload = driver.blocks.get_by_right(&block.block_hash);
            assert!(payload.is_some(), "unknown block hash");
            (block.block_number, payload.cloned())
        }
    };
    (latest_block_height, latest_block_payload)
}

async fn get_proposals(driver: &EmeraldDriver, state: &State) -> BTreeSet<Proposal> {
    let mut proposals = BTreeSet::new();

    // Get decided proposals
    for height in 1..state.current_height.as_u64() {
        if let Some(decided) = state
            .store
            .get_decided_value(EmeraldHeight::new(height))
            .await
            .expect("Failed to get decided value")
        {
            let proposal = driver
                .proposals
                .get_by_right(&decided.value.id())
                .expect("Unknown proposed value")
                .clone();
            proposals.insert(proposal);
        }
    }

    // Get pending proposals
    let pending_parts = state
        .store
        .get_pending_proposal_parts(state.current_height, state.current_round)
        .await
        .expect("Failed to read pending proposal parst");

    for parts in pending_parts {
        let (proposed_value, _) = assemble_value_from_parts(parts);
        let proposal = driver
            .proposals
            .get_by_right(&proposed_value.value.id())
            .expect("Unknown proposed value")
            .clone();
        proposals.insert(proposal);
    }

    // Get undecided proposals
    let undecided_proposals = state
        .store
        .get_undecided_proposals(state.current_height, state.current_round)
        .await
        .expect("Failed to read undecided proposals");

    for proposed_value in undecided_proposals {
        let proposal = driver
            .proposals
            .get_by_right(&proposed_value.value.id())
            .expect("Unknown proposed value")
            .clone();
        proposals.insert(proposal);
    }

    proposals
}
