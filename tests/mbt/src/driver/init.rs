use std::path::PathBuf;

use anyhow::{anyhow, Result};
use emerald::node::{App as EmeraldApp, StateComponents};
use malachitebft_app_channel::app::types::core::VotingPower;
use malachitebft_eth_cli::config::{Config, ConsensusConfig, ElNodeType, EmeraldConfig, P2pConfig};
use malachitebft_eth_types::secp256k1::PrivateKey;
use malachitebft_eth_types::utils::validators::make_validators_with_individual_seeds;
use malachitebft_eth_types::{Address, Height as EmeraldHeight, Validator};
use tempfile::TempDir;
use tokio::time::Duration;

use crate::driver::rt::Runtime;
use crate::driver::EmeraldDriver;
use crate::state::Node;
use crate::sut::Sut;
use crate::{NODES, N_NODES};

impl EmeraldDriver {
    pub fn init(&mut self) -> Result<()> {
        // Reset the driver
        *self = Default::default();
        self.runtime.replace(Runtime::new()?);

        // Initialize all nodes
        let validators = Self::validators();

        for (node_idx, node) in NODES.iter().enumerate() {
            let node = node.to_string();
            let (validator, private_key) = &validators[node_idx];
            self.init_node(node_idx, node, validator, private_key)?;
        }

        Ok(())
    }

    pub fn node_crash(&mut self, node: Node) -> Result<()> {
        if let Some(app) = self.sut.remove(&node) {
            drop(app); // stop emerald node
        }

        let validators = Self::validators();
        let node_idx = NODES
            .iter()
            .position(|n| n == &node)
            .ok_or(anyhow!("Unknown node: {}", node))?;

        let (validator, private_key) = &validators[node_idx];
        self.init_node(node_idx, node, validator, private_key)?;

        Ok(())
    }

    fn validators() -> [(Validator, PrivateKey); N_NODES] {
        let voting_power = std::array::repeat::<VotingPower, N_NODES>(1);
        make_validators_with_individual_seeds(voting_power)
    }

    fn init_node(
        &mut self,
        node_idx: usize,
        node: String,
        validator: &Validator,
        private_key: &PrivateKey,
    ) -> Result<()> {
        let address = Address::from_public_key(&validator.public_key);
        let components = self.init_app_state(&node, node_idx, private_key)?;
        self.history.record_address(node.clone(), address);

        self.sut.insert(
            node.to_string(),
            Sut {
                components,
                address,
            },
        );

        Ok(())
    }

    fn init_app_state(
        &mut self,
        node: &String,
        node_idx: usize,
        private_key: &PrivateKey,
    ) -> Result<StateComponents> {
        let rt = self
            .runtime
            .as_ref()
            .ok_or(anyhow!("Uninitialized runtime"))?;

        let app = Self::setup_app(&rt.tempdir, node, node_idx, private_key)?;

        rt.block_on(app.build_state())
            .map_err(|err| anyhow!("Failed to build state components: {:?}", err))
    }

    fn setup_app(
        tempdir: &TempDir,
        node: &Node,
        node_idx: usize,
        private_key: &PrivateKey,
    ) -> Result<EmeraldApp> {
        let config = Self::create_test_config(node, node_idx as u16)?;
        let (home_dir, genesis_file, emerald_config_file, private_key_file) =
            Self::setup_test_files(tempdir, node, private_key)?;

        Ok(EmeraldApp {
            config,
            home_dir,
            genesis_file,
            emerald_config_file,
            private_key_file,
            start_height: Some(EmeraldHeight::new(0)),
        })
    }

    fn create_test_config(node: &Node, port_offset: u16) -> Result<Config> {
        let consensus = P2pConfig {
            listen_addr: format!("/ip4/127.0.0.1/tcp/{}", 27000 + port_offset).parse()?,
            ..Default::default()
        };

        Ok(Config {
            moniker: node.clone(),
            consensus: ConsensusConfig {
                p2p: consensus,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    fn setup_test_files(
        tempdir: &TempDir,
        node: &Node,
        private_key: &PrivateKey,
    ) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf)> {
        let home_dir = tempdir.path().join(node);
        if home_dir.exists() {
            std::fs::remove_dir_all(&home_dir)?;
        }
        std::fs::create_dir_all(&home_dir)?;

        let genesis_file = home_dir.join("genesis.json");
        let emerald_config_file = home_dir.join("emerald.toml");
        let private_key_file = home_dir.join("priv_validator_key.json");

        let private_key_json = serde_json::to_string_pretty(private_key)?;
        std::fs::write(&private_key_file, private_key_json)?;

        // Use shared genesis from assets directory. This must match what reth
        // is using and contain all validators.
        let shared_genesis_path =
            std::env::current_dir()?.join("../../assets/emerald_genesis.json");

        let genesis_json = std::fs::read_to_string(&shared_genesis_path)?;
        std::fs::write(&genesis_file, genesis_json)?;

        // Create test emerald config
        let emerald_config = Self::create_test_emerald_config(node)?;
        let emerald_config_toml = toml::to_string_pretty(&emerald_config)?;
        std::fs::write(&emerald_config_file, emerald_config_toml)?;

        Ok((
            home_dir,
            genesis_file,
            emerald_config_file,
            private_key_file,
        ))
    }

    fn create_test_emerald_config(moniker: &Node) -> Result<EmeraldConfig> {
        // Use the JWT secret from the assets directory. This is created by reth module.
        let jwt_path = std::env::current_dir()?
            .join("../../assets/jwtsecret")
            .canonicalize()?;

        Ok(EmeraldConfig {
            moniker: moniker.to_string(),
            engine_authrpc_address: "http://localhost:8551".to_string(),
            execution_authrpc_address: "http://localhost:8545".to_string(),
            jwt_token_path: jwt_path.to_string_lossy().to_string(),
            min_block_time: Duration::from_millis(100),
            max_retain_blocks: 100,
            prune_at_block_interval: 10,
            retry_config: Default::default(),
            el_node_type: ElNodeType::Archive,
        })
    }
}
