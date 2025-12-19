use std::path::PathBuf;

use emerald::node::{App, StateComponents};
use malachitebft_app_channel::app::types::core::VotingPower;
use malachitebft_eth_cli::config::{Config, ConsensusConfig, ElNodeType, EmeraldConfig, P2pConfig};
use malachitebft_eth_types::secp256k1::PrivateKey;
use malachitebft_eth_types::utils::validators::make_validators_with_individual_seeds;
use malachitebft_eth_types::{Address, Height as EmeraldHeight, Validator};
use tempfile::TempDir;
use tokio::time::Duration;

use crate::driver::EmeraldDriver;
use crate::state::Node;
use crate::{NODES, N_NODES};

impl EmeraldDriver {
    pub fn init(&mut self) {
        // Reset driver state. Note that it's okay to reuse the Tokio runtime.
        self.nodes.clear();
        self.addresses.clear();
        self.proposals.clear();
        self.values.clear();
        self.streams.clear();
        self.blocks.clear();

        let tempdir = TempDir::with_prefix("mbt-emerald-app")
            .expect("Failed to create temporary folder for MBT");

        self.tempdir.replace(tempdir); // drop/delete old dir, keep the new one.

        let validators = Self::validators();

        for (node_idx, node) in NODES.iter().enumerate() {
            let node = node.to_string();
            let (validator, private_key) = &validators[node_idx];
            self.init_node(node_idx, node, validator, private_key);
        }
    }

    pub fn node_crash(&mut self, node: Node) {
        let app = self.nodes.remove(&node).expect("Unknown node");
        drop(app); // stop emerald node

        let validators = Self::validators();
        let node_idx = NODES.iter().position(|n| n == &node).expect("Unknown node");
        let (validator, private_key) = &validators[node_idx];
        self.init_node(node_idx, node, validator, private_key);
    }

    fn validators() -> [(Validator, PrivateKey); N_NODES] {
        let voting_power = std::array::repeat::<VotingPower, N_NODES>(1);
        make_validators_with_individual_seeds(voting_power)
    }

    fn init_node(
        &mut self,
        i: usize,
        node: String,
        validator: &Validator,
        private_key: &PrivateKey,
    ) {
        let address = Address::from_public_key(&validator.public_key);
        self.addresses.insert(node.clone(), address);

        let state = self.init_app_state(&node, i, private_key);
        self.nodes.insert(node.to_string(), state);
    }

    fn init_app_state(
        &mut self,
        node: &String,
        node_idx: usize,
        private_key: &PrivateKey,
    ) -> StateComponents {
        let tempdir = self.tempdir.as_ref().expect("Temp dir can't be None");
        let app = Self::setup_app(tempdir, node, node_idx, private_key);
        self.runtime
            .block_on(async { app.build_state().await })
            .expect("Failed to build state")
    }

    fn setup_app(tempdir: &TempDir, node: &Node, node_idx: usize, private_key: &PrivateKey) -> App {
        let config = Self::create_test_config(node, node_idx as u16);
        let (home_dir, genesis_file, emerald_config_file, private_key_file) =
            Self::setup_test_files(tempdir, node, private_key);

        App {
            config,
            home_dir,
            genesis_file,
            emerald_config_file,
            private_key_file,
            start_height: Some(EmeraldHeight::new(0)),
        }
    }

    fn create_test_config(node: &Node, port_offset: u16) -> Config {
        let consensus = P2pConfig {
            listen_addr: format!("/ip4/127.0.0.1/tcp/{}", 27000 + port_offset)
                .parse()
                .expect("Failed to parse consensus address"),
            ..Default::default()
        };
        Config {
            moniker: node.clone(),
            consensus: ConsensusConfig {
                p2p: consensus,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn setup_test_files(
        tempdir: &TempDir,
        node: &Node,
        private_key: &PrivateKey,
    ) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
        let home_dir = tempdir.path().join(node);
        if home_dir.exists() {
            std::fs::remove_dir_all(&home_dir).expect("Failed to cleanup emerald home directory");
        }
        std::fs::create_dir_all(&home_dir).expect("Failed to create emerald home directory");

        let genesis_file = home_dir.join("genesis.json");
        let emerald_config_file = home_dir.join("emerald.toml");
        let private_key_file = home_dir.join("priv_validator_key.json");

        let private_key_json =
            serde_json::to_string_pretty(private_key).expect("Failed to serialize private key");
        std::fs::write(&private_key_file, private_key_json).expect("Failed to write private key");

        // Use shared genesis from assets directory. This must match what reth
        // is using and contain all validators.
        let shared_genesis_path = std::env::current_dir()
            .expect("Failed to get current directory")
            .join("../../assets/emerald_genesis.json");

        let genesis_json = std::fs::read_to_string(&shared_genesis_path)
            .expect("Failed to read shared genesis file. Make sure you ran start-reth.sh first.");
        std::fs::write(&genesis_file, genesis_json).expect("Failed to write genesis file");

        // Create test emerald config
        let emerald_config = Self::create_test_emerald_config(node);
        let emerald_config_toml =
            toml::to_string_pretty(&emerald_config).expect("Failed to serialize emerald config");
        std::fs::write(&emerald_config_file, emerald_config_toml)
            .expect("Failed to write emerald config");

        (
            home_dir,
            genesis_file,
            emerald_config_file,
            private_key_file,
        )
    }

    fn create_test_emerald_config(moniker: &Node) -> EmeraldConfig {
        // Use the JWT secret from the assets directory. This should match what
        // start-reth.sh uses.
        let jwt_path = std::env::current_dir()
            .expect("Failed to get current directory")
            .join("../../assets/jwtsecret")
            .canonicalize()
            .expect("Failed to find jwtsecret");

        EmeraldConfig {
            moniker: moniker.to_string(),
            engine_authrpc_address: "http://localhost:8551".to_string(),
            execution_authrpc_address: "http://localhost:8545".to_string(),
            jwt_token_path: jwt_path.to_string_lossy().to_string(),
            min_block_time: Duration::from_millis(100),
            max_retain_blocks: 100,
            prune_at_block_interval: 10,
            retry_config: Default::default(),
            el_node_type: ElNodeType::Archive,
        }
    }
}
