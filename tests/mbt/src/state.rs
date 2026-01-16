//! This module maps Quint and Emerald states. Only relevant parts of the
//! Emerald state are captured. Quint Connect is responsible for comparing the
//! extracted data with the Quint state.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::bail;
use emerald::state::assemble_value_from_parts;
use emerald::State;
use itf::de::{self, As};
use malachitebft_app_channel::app::types::core::Round as EmeraldRound;
use malachitebft_eth_types::Height as EmeraldHeight;
use quint_connect::Result;
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
    pub last_decided_height: Height,
    #[serde(with = "As::<de::Option::<_>>")]
    pub last_decided_payload: Option<Payload>,
    pub proposals: BTreeSet<Proposal>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(tag = "tag")]
pub enum FailureMode {
    NodeCrash,
    NodeRestart,
    ProcessRestart,
    ConsensusTimeout,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct SpecState(pub BTreeMap<Node, NodeState>);

impl quint_connect::State<EmeraldDriver> for SpecState {
    fn from_driver(driver: &EmeraldDriver) -> Result<Self> {
        let mut system = BTreeMap::new();
        let Some(rt) = &driver.runtime else {
            bail!("Uninitialized runtime");
        };

        for (node, app) in &driver.sut {
            let state = &app.components.state;
            let proposals = rt.block_on(get_proposals(driver, state))?;
            let (last_decided_height, last_decided_payload) = get_latest_block_info(driver, state)?;

            let spec_state = NodeState {
                current_height: state.current_height.as_u64(),
                current_round: state.current_round.as_u32().unwrap_or(0),
                last_decided_height,
                last_decided_payload,
                proposals,
            };

            system.insert(node.clone(), spec_state);
        }

        Ok(Self(system))
    }
}

fn get_latest_block_info(
    driver: &EmeraldDriver,
    state: &State,
) -> Result<(Height, Option<Payload>)> {
    match &state.latest_block {
        None => Ok((0, None)),                                   // genesis
        Some(block) if block.block_number == 0 => Ok((0, None)), // genesis
        Some(block) => {
            let payload = driver.history.get_payload(&block.block_hash)?;
            Ok((block.block_number, Some(payload)))
        }
    }
}

async fn get_proposals(driver: &EmeraldDriver, state: &State) -> Result<BTreeSet<Proposal>> {
    let mut proposals = BTreeSet::new();
    let mut max_height = 1;
    let mut max_round = 0;

    for (proposal, _) in &driver.history.proposals {
        max_height = max_height.max(proposal.height);
        max_round = max_round.max(proposal.round);
    }

    for height in 1..=max_height {
        let height = EmeraldHeight::new(height);

        if let Some(decided) = state.store.get_decided_value(height).await? {
            let height = decided.certificate.height;
            let round = decided.certificate.round;
            let value_id = decided.certificate.value_id;
            let proposal = driver.history.get_proposal(height, round, value_id)?;
            proposals.insert(proposal);
        }

        for round in 0..=max_round {
            let round = EmeraldRound::new(round);
            let undecided_proposals = state.store.get_undecided_proposals(height, round).await?;

            for proposed_value in undecided_proposals {
                let height = proposed_value.height;
                let round = proposed_value.round;
                let value_id = proposed_value.value.id();
                let proposal = driver.history.get_proposal(height, round, value_id)?;
                proposals.insert(proposal.clone());
            }

            let pending_parts = state
                .store
                .get_pending_proposal_parts(height, round)
                .await?;

            for parts in pending_parts {
                let (proposed_value, _) = assemble_value_from_parts(parts);
                let height = proposed_value.height;
                let round = proposed_value.round;
                let value_id = proposed_value.value.id();
                let proposal = driver.history.get_proposal(height, round, value_id)?;
                proposals.insert(proposal);
            }
        }
    }

    Ok(proposals)
}
