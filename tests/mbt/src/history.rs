use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use bimap::BiMap;
use malachitebft_app_channel::app::streaming::StreamMessage;
use malachitebft_app_channel::app::types::core::Round as EmeraldRound;
use malachitebft_eth_engine::json_structures::ExecutionBlock;
use malachitebft_eth_types::{
    Address, BlockHash, Height as EmeraldHeight, ProposalPart, Value, ValueId as EmeraldValueId,
};

use crate::state::{Node, Payload, Proposal, ValueId};

#[derive(Default)]
pub struct History {
    pub addresses: BiMap<Node, Address>,
    pub proposals: BiMap<Proposal, (EmeraldHeight, EmeraldRound, EmeraldValueId)>,
    pub streams: BTreeMap<ValueId, Vec<StreamMessage<ProposalPart>>>,
    pub values: BTreeMap<ValueId, Value>,
    pub blocks: BiMap<Payload, BlockHash>,
}

impl History {
    pub fn get_address(&self, node: &Node) -> Result<Address> {
        self.addresses
            .get_by_left(node)
            .cloned()
            .ok_or(anyhow!("Can't find address for node: {}", node))
    }

    pub fn get_payload(&self, hash: &BlockHash) -> Result<Payload> {
        self.blocks
            .get_by_right(hash)
            .cloned()
            .ok_or(anyhow!("Can't find payload for hash: {}", hash))
    }

    pub fn get_proposal(
        &self,
        height: EmeraldHeight,
        round: EmeraldRound,
        value_id: EmeraldValueId,
    ) -> Result<Proposal> {
        self.proposals
            .get_by_right(&(height, round, value_id))
            .cloned()
            .ok_or(anyhow!("No proposal for value id: {}", value_id))
    }

    pub fn get_stream(&self, value_id: &ValueId) -> Result<Vec<StreamMessage<ProposalPart>>> {
        self.streams
            .get(value_id)
            .cloned()
            .ok_or(anyhow!("No stream parts for value id: {:?}", value_id))
    }

    pub fn get_value(&self, value_id: &ValueId) -> Result<Value> {
        self.values
            .get(value_id)
            .cloned()
            .ok_or(anyhow!("No value for id: {:?}", value_id))
    }

    pub fn get_value_id(&self, proposal: &Proposal) -> Result<EmeraldValueId> {
        let (_, _, value_id) = self
            .proposals
            .get_by_left(proposal)
            .cloned()
            .ok_or(anyhow!("No value id for proposal: {:?}", proposal))?;
        Ok(value_id)
    }

    pub fn record_address(&mut self, node: Node, address: Address) {
        self.addresses.insert(node, address);
    }

    pub fn record_proposal(
        &mut self,
        proposal: Proposal,
        value: Value,
        stream: Vec<StreamMessage<ProposalPart>>,
    ) {
        let height = EmeraldHeight::new(proposal.height);
        let round = EmeraldRound::new(proposal.round);
        let value_id = value.id();

        self.values.insert(proposal.id(), value);
        self.streams.insert(proposal.id(), stream);
        self.proposals.insert(proposal, (height, round, value_id));
    }

    pub fn record_block(&mut self, proposal: Proposal, block: ExecutionBlock) {
        self.blocks.insert(proposal.payload, block.block_hash);
    }
}
