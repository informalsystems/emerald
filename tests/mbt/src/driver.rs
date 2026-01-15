mod environment;
mod utils;

use std::collections::BTreeMap;

use quint_connect::{switch, Result, Step};

use crate::history::History;
use crate::runtime::Runtime;
use crate::state::{Node, SpecState};
use crate::sut::{self, Sut};

/// Model-based testing driver for Emerald.
///
/// Connects a Quint formal specification to the Emerald implementation,
/// executing actions taken on the system under test (SUT) while tracking
/// execution history for verification.
#[derive(Default)]
pub struct EmeraldDriver {
    pub sut: BTreeMap<Node, Sut>,
    pub history: History,
    pub runtime: Option<Runtime>,
}

impl quint_connect::Driver for EmeraldDriver {
    type State = SpecState;

    fn config() -> quint_connect::Config {
        quint_connect::Config {
            state: &["emerald::choreo::s", "system"],
            nondet: &["emerald::choreo::s", "extensions", "action_taken"],
        }
    }

    /// Called for each action taken in the Quint trace. It maps the Quint
    /// action and its associated values with the equivalent code in the SUT.
    fn step(&mut self, step: &Step) -> Result {
        switch!(step {
            InitAction => {
                self.init()?;
            },
            ConsensusReadyAction(node) => {
                self.perform(node, |app, _|
                    app.consensus_ready()
                )?;
            },
            StartedRoundAction(node, height, round, proposer) => {
                self.perform(node, |app, hist|
                    app.started_round(hist, height, round, proposer)
                )?;
            },
            GetValueAction(node, height, round, proposal) => {
                self.perform(node, |app, hist|
                    app.get_value(hist, height, round, proposal)
                )?;
            },
            ReceivedProposalAction(node, proposal) => {
                self.perform(node, |app, hist|
                    app.receive_proposal(hist, proposal)
                )?;
            },
            ProcessSyncedValueAction(node, proposal) => {
                self.perform(node, |app, hist|
                    app.process_synced_value(hist, proposal)
                )?;
            },
            DecidedAction(node, proposal) => {
                let votes = sut::mock_votes(&self.sut, &self.history, &proposal)?;
                self.perform(node, |app, hist|
                    app.decided(hist, proposal, votes)
                )?;
            },
            GetDecidedValueAction(node, height, proposal?) => {
                self.perform(node, |app, hist|
                    app.get_decided(hist, height, proposal)
                )?;
            },
            Failure(node, mode) => {
                self.failure(node, mode)?;
            }
        })
    }
}
