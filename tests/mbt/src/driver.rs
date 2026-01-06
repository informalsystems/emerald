mod consensus_ready;
mod handle_decided;
mod handle_get_value;
mod handle_process_synced_value;
mod handle_received_proposal;
mod handle_started_round;
mod init;

use std::collections::BTreeMap;

use bimap::BiMap;
use emerald::app::process_consensus_message;
use emerald::node::StateComponents;
use malachitebft_app_channel::app::streaming::StreamMessage;
use malachitebft_app_channel::AppMsg;
use malachitebft_eth_types::{
    Address, BlockHash, EmeraldContext, ProposalPart, Value, ValueId as EmeraldValueId,
};
use quint_connect::{switch, Driver, Result, Step};
use tempfile::TempDir;

use crate::reth::{self, RethHandle};
use crate::state::{Node, Payload, Proposal, SpecState, ValueId};

pub struct EmeraldDriver {
    // SUT: the concrete emerald state componenets.
    pub nodes: BTreeMap<Node, StateComponents>,
    pub addresses: BiMap<Node, Address>,
    // Historical variables tracked during trace replay.
    pub proposals: BiMap<Proposal, EmeraldValueId>,
    pub values: BTreeMap<ValueId, Value>,
    pub streams: BTreeMap<ValueId, Vec<StreamMessage<ProposalPart>>>,
    pub blocks: BiMap<Payload, BlockHash>,
    // Necessary runtime and handles to interact with Reth and Emerald.
    pub runtime: tokio::runtime::Runtime,
    pub tempdir: Option<TempDir>,
    // TODO: have a reth instace per emerald node.
    _reth: RethHandle,
}

impl Default for EmeraldDriver {
    fn default() -> Self {
        let reth = reth::start().expect("Failed to start RETH");

        Self {
            nodes: BTreeMap::new(),
            addresses: BiMap::new(),
            proposals: BiMap::new(),
            values: BTreeMap::new(),
            streams: BTreeMap::new(),
            blocks: BiMap::new(),
            runtime: tokio::runtime::Runtime::new().expect("Failed to create tokio runtime"),
            tempdir: None,
            _reth: reth,
        }
    }
}

impl Driver for EmeraldDriver {
    type State = SpecState;

    fn config() -> quint_connect::Config {
        quint_connect::Config {
            state: &["emerald_app::choreo::s", "system"],
            nondet: &["emerald_app::choreo::s", "extensions", "action_taken"],
        }
    }

    fn step(&mut self, step: &Step) -> Result {
        switch!(step {
            InitAction => {
                self.init()
            },
            ConsensusReadyAction(node) => {
                self.handle_consensus_ready(node)
            },
            StartedRoundAction(node, height, round, proposer) => {
                self.handle_started_round(node, height, round, proposer)
            },
            GetValueAction(node, height, round, proposal) => {
                self.handle_get_value(node, height, round, proposal)
            },
            ReceivedProposalAction(node, proposal) => {
                self.handle_received_proposal(node, proposal)
            },
            ProcessSyncedValueAction(node, proposal) => {
                self.handle_process_synced_value(node, proposal)
            },
            DecidedAction(node, proposal) => {
                self.handle_decided(node, proposal)
            },
            NodeCrash(node) => {
                self.node_crash(node)
            }
        })
    }
}

async fn process_app_message(app: &mut StateComponents, msg: AppMsg<EmeraldContext>) {
    process_consensus_message(
        msg,
        &mut app.state,
        &mut app.channels,
        &app.engine,
        &app.emerald_config,
    )
    .await
    .expect("Failed to process consensus message");
}
