use core::time::Duration;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use emerald::node::{App as EmeraldApp, AppRuntime};
use malachitebft_eth_cli::config;
use malachitebft_eth_types::{Address, Height as EmeraldHeight};
use tempfile::TempDir;

use crate::driver::EmeraldDriver;
use crate::runtime::Runtime;
use crate::state::{FailureMode, Node};
use crate::sut::Sut;
use crate::{reth, NODES};

impl EmeraldDriver {
    /// Initializes the test environment by resetting the driver state and
    /// creating one Emerald and Reth instance per [crate::NODES].
    pub fn init(&mut self) -> Result<()> {
        // Reset the driver
        *self = Self::default();
        reth::recreate_all()?;

        // Fresh test directory for the new environment
        let test_dir = TempDir::with_prefix("emerald-mbt")?;

        for (node_idx, node) in NODES.iter().enumerate() {
            let node = node.to_string();
            let home_dir = TempDir::with_suffix_in(&node, &test_dir)?;
            self.init_node(node_idx, node, home_dir)?;
        }

        self.runtime.replace(Runtime::new(test_dir)?);
        Ok(())
    }

    /// Simulates a failure on the given node according to the failure mode:
    ///
    /// - [FailureMode::ConsensusTimeout]: No-op, model provides next inputs
    /// - [FailureMode::NodeCrash]: Recreates Reth and Emerald
    /// - [FailureMode::NodeRestart]: Restarts Reth and Emerald
    /// - [FailureMode::ProcessRestart]: Restarts Emerald only
    pub fn failure(&mut self, node: Node, mode: FailureMode) -> Result<()> {
        if let FailureMode::ConsensusTimeout = mode {
            // Noop wrt. MBT. The model will follow up with the proper consensus
            // input later.
            return Ok(());
        }

        let node_idx = NODES
            .iter()
            .position(|&other| node == other)
            .ok_or(anyhow!("Unknown node: {node}"))?;

        let home_dir = match mode {
            // Both Emerald and Reth crashed and lost their data. Recreate Reth
            // and return a new empty home dir for Emerald to start from.
            FailureMode::NodeCrash => {
                self.stop_node(&node)?;
                reth::recreate(node_idx)?;
                TempDir::with_suffix_in(
                    &node,
                    self.runtime
                        .as_ref()
                        .map(|rt| rt.temp_dir.path())
                        .ok_or(anyhow!("Uninitialized test dir"))?,
                )?
            }
            // Both Emerald and Reth restarted without data loss. Restart
            // Emerald and return the previous home dir for Emerald to start
            // from.
            FailureMode::NodeRestart => {
                let home_dir = self.stop_node(&node)?;
                reth::restart(node_idx)?;
                home_dir
            }
            // Just Emerald restarted without data loss. Return the previous
            // home dir for Emerald to start form.
            FailureMode::ProcessRestart => self.stop_node(&node)?,
            FailureMode::ConsensusTimeout => unreachable!(),
        };

        self.init_node(node_idx, node, home_dir)?;
        Ok(())
    }

    fn init_node(&mut self, node_idx: usize, node: String, home_dir: TempDir) -> Result<()> {
        let home_path = home_dir.path().to_path_buf();
        let runtime = Runtime::new(home_dir)?;
        let components = runtime.block_on(self.init_app_state(node_idx, home_path))?;

        let public_key = components.state.signing_provider.private_key().public_key();
        let address = Address::from_public_key(&public_key);
        self.history.record_address(node.clone(), address);

        self.sut.insert(
            node,
            Sut {
                components,
                address,
                runtime,
            },
        );

        Ok(())
    }

    async fn init_app_state(&mut self, node_idx: usize, home_dir: PathBuf) -> Result<AppRuntime> {
        let mut app = Self::setup_app(node_idx, home_dir)?;

        // Disable unecessary components since Malachite is being mocked for MBT.
        app.config.consensus.enabled = false;
        app.config.value_sync.enabled = false;
        app.config.metrics.enabled = false;

        app.build_runtime()
            .await
            .map_err(|err| anyhow!("Failed to build app state: {err:?}"))
    }

    fn setup_app(node_idx: usize, home_dir: PathBuf) -> Result<EmeraldApp> {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()?;

        let emerald_config_file = project_root
            .join(".testnet/config")
            .join(node_idx.to_string())
            .join("config.toml");

        let nodes_path = project_root.join("nodes").join(node_idx.to_string());
        let config_file = nodes_path.join("config/config.toml");
        let genesis_file = nodes_path.join("config/genesis.json");
        let private_key_file = nodes_path.join("config/priv_validator_key.json");

        let config = config::load_config(&config_file, None)
            .map_err(|err| anyhow!("Failed to load config file: {err}"))?;

        Ok(EmeraldApp {
            config,
            home_dir,
            genesis_file,
            emerald_config_file,
            private_key_file,
            start_height: Some(EmeraldHeight::new(0)),
        })
    }

    fn stop_node(&mut self, node: &Node) -> Result<TempDir> {
        // NOTE: by removing the SUT from the driver state we let its components
        // drop and clean up. We only return the node's `temp_dir` so that the
        // caller can decide to reuse it or not.
        let sut = self
            .sut
            .remove(node)
            .ok_or(anyhow!("No SUT for node: {node}"))?;

        let Runtime { temp_dir, tokio } = sut.runtime;

        // NOTE: we attempt to wait for its tasks to get completed before
        // returning the `temp_dir` so that no advisory locks (libc's flock) are
        // held on Malachite's WAL.
        tokio.shutdown_timeout(Duration::from_secs(5));

        Ok(temp_dir)
    }
}
