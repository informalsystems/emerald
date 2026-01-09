use std::path::PathBuf;

use anyhow::{anyhow, Result};
use emerald::node::{App as EmeraldApp, StateComponents};
use malachitebft_eth_cli::config;
use malachitebft_eth_types::{Address, Height as EmeraldHeight};
use tempfile::TempDir;

use crate::driver::rt::Runtime;
use crate::driver::EmeraldDriver;
use crate::state::Node;
use crate::sut::Sut;
use crate::{reth, NODES};

impl EmeraldDriver {
    pub fn init(&mut self) -> Result<()> {
        // Reset the driver
        *self = Default::default();
        self.runtime.replace(Runtime::new()?);
        reth::recreate_all()?;

        for (node_idx, node) in NODES.iter().enumerate() {
            let node = node.to_string();
            self.init_node(node_idx, node)?;
        }

        Ok(())
    }

    pub fn node_crash(&mut self, node: Node) -> Result<()> {
        if let Some(sut) = self.sut.remove(&node) {
            drop(sut); // stop emerald node
        }

        let node_idx = NODES
            .iter()
            .position(|n| n == &node)
            .ok_or(anyhow!("Unknown node: {}", node))?;

        reth::recreate(node_idx)?;
        self.init_node(node_idx, node)?;

        Ok(())
    }

    fn init_node(&mut self, node_idx: usize, node: String) -> Result<()> {
        let (components, home_dir) = self.init_app_state(&node, node_idx)?;

        let public_key = components.state.signing_provider.private_key().public_key();
        let address = Address::from_public_key(&public_key);
        self.history.record_address(node.clone(), address);

        self.sut.insert(
            node.to_string(),
            Sut {
                components,
                address,
                home_dir,
            },
        );

        Ok(())
    }

    fn init_app_state(
        &mut self,
        node: &String,
        node_idx: usize,
    ) -> Result<(StateComponents, TempDir)> {
        let rt = self
            .runtime
            .as_ref()
            .ok_or(anyhow!("Uninitialized runtime"))?;

        let home_dir = TempDir::with_suffix_in(node, &rt.temp_dir)?;
        let mut app = Self::setup_app(node_idx, home_dir.path().to_path_buf())?;

        // Disable unecessary components since malachite is being moded for MBT.
        app.config.consensus.enabled = false;
        app.config.value_sync.enabled = false;
        app.config.metrics.enabled = false;

        let app = rt
            .block_on(app.build_state())
            .map_err(|err| anyhow!("Failed to build state components: {:?}", err))?;

        Ok((app, home_dir))
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
            .map_err(|err| anyhow!("Failed to load config file: {}", err))?;

        Ok(EmeraldApp {
            config,
            home_dir,
            genesis_file,
            emerald_config_file,
            private_key_file,
            start_height: Some(EmeraldHeight::new(0)),
        })
    }
}
