mod init;
mod perform;
mod rt;

use std::collections::BTreeMap;

use quint_connect::{switch, Driver, Result, Step};

use crate::driver::rt::Runtime;
use crate::hist::History;
use crate::state::{Node, SpecState};
use crate::sut::{self, Sut};

#[derive(Default)]
pub struct EmeraldDriver {
    pub sut: BTreeMap<Node, Sut>,
    pub history: History,
    pub runtime: Option<Runtime>,
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
                self.init()?
            },
            ConsensusReadyAction(node) => {
                self.perform(node, |app, _| app.consensus_ready())?
            },
            StartedRoundAction(node, height, round, proposer) => {
                self.perform(node, |app, hist| app.started_round(hist, height, round, proposer))?
            },
            GetValueAction(node, height, round, proposal) => {
                self.perform(node, |app, hist| app.get_value(hist, height, round, proposal))?
            },
            ReceivedProposalAction(node, proposal) => {
                self.perform(node, |app, hist| app.receive_proposal(hist, proposal))?
            },
            ProcessSyncedValueAction(node, proposal) => {
                self.perform(node, |app, hist| app.process_synced_value(hist, proposal))?
            },
            DecidedAction(node, proposal) => {
                let votes = sut::mock_votes(&self.sut, &self.history, &proposal)?;
                self.perform(node, |app, hist| app.decided(hist, proposal, votes))?
            },
            GetDecidedValueAction(node, height, proposal?) => {
                self.perform(node, |app, hist| app.get_decided(hist, height, proposal))?
            },
            NodeCrash(node) => {
                self.node_crash(node)?
            }
        })
    }
}
