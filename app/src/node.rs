//! The Application (or Node) definition. The Node trait implements the Consensus context and the
//! cryptographic library used for signing.

use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use async_trait::async_trait;
use color_eyre::eyre;
use malachitebft_app_channel::app::events::{RxEvent, TxEvent};
use malachitebft_app_channel::app::node::NodeHandle;
use malachitebft_eth_engine::engine::Engine;
use malachitebft_eth_engine::engine_rpc::EngineRPC;
use malachitebft_eth_engine::ethereum_rpc::EthereumRPC;
use rand::{CryptoRng, RngCore};

use malachitebft_app_channel::app::metrics::SharedRegistry;
use malachitebft_app_channel::app::node::{
    CanGeneratePrivateKey, CanMakeGenesis, CanMakePrivateKeyFile, EngineHandle, Node,
};
use malachitebft_app_channel::app::types::core::VotingPower;
use malachitebft_app_channel::app::types::Keypair;
use malachitebft_eth_cli::config::{Config, MalakethConfig};

// Use the same types used for integration tests.
// A real application would use its own types and context instead.
use malachitebft_eth_cli::metrics;
use malachitebft_eth_types::codec::proto::ProtobufCodec;
use malachitebft_eth_types::{
    Address, Ed25519Provider, Genesis, Height, MalakethContext, PrivateKey, PublicKey, Validator,
    ValidatorSet,
};
use tokio::task::JoinHandle;
use url::Url;

use crate::metrics::DbMetrics;
use crate::state::State;
use crate::store::Store;

/// Main application struct implementing the consensus node functionality
#[derive(Clone)]
pub struct App {
    pub config: Config,
    pub home_dir: PathBuf,
    pub genesis_file: PathBuf,
    pub malaketh_config_file: PathBuf,
    pub private_key_file: PathBuf,
    pub start_height: Option<Height>,
}

impl App {
    fn load_malaketh_config(&self) -> eyre::Result<MalakethConfig> {
        let malaketh_config_content =
            fs::read_to_string(&self.malaketh_config_file).map_err(|e| {
                eyre::eyre!(
                    "Failed to read malaketh config file `{}`: {e}",
                    self.malaketh_config_file.display()
                )
            })?;
        let malaketh_config =
            toml::from_str::<crate::config::MalakethConfig>(&malaketh_config_content)
                .map_err(|e| eyre::eyre!("Failed to parse malaketh config file: {e}"))?;
        Ok(malaketh_config)
    }
}

pub struct Handle {
    pub app: JoinHandle<()>,
    pub engine: EngineHandle,
    pub tx_event: TxEvent<MalakethContext>,
}

#[async_trait]
impl NodeHandle<MalakethContext> for Handle {
    fn subscribe(&self) -> RxEvent<MalakethContext> {
        self.tx_event.subscribe()
    }

    async fn kill(&self, _reason: Option<String>) -> eyre::Result<()> {
        self.engine.actor.kill_and_wait(None).await?;
        self.app.abort();
        self.engine.handle.abort();
        Ok(())
    }
}

#[async_trait]
impl Node for App {
    type Context = MalakethContext;
    type Config = Config;
    type Genesis = Genesis;
    type PrivateKeyFile = PrivateKey;
    type SigningProvider = Ed25519Provider;
    type NodeHandle = Handle;

    fn get_home_dir(&self) -> PathBuf {
        self.home_dir.to_owned()
    }

    fn load_config(&self) -> eyre::Result<Self::Config> {
        Ok(self.config.clone())
    }

    fn get_signing_provider(&self, private_key: PrivateKey) -> Self::SigningProvider {
        Ed25519Provider::new(private_key)
    }

    fn get_address(&self, pk: &PublicKey) -> Address {
        Address::from_public_key(pk)
    }

    fn get_public_key(&self, pk: &PrivateKey) -> PublicKey {
        pk.public_key()
    }

    fn get_keypair(&self, pk: PrivateKey) -> Keypair {
        Keypair::ed25519_from_bytes(pk.inner().to_bytes()).unwrap()
    }

    fn load_private_key(&self, file: Self::PrivateKeyFile) -> PrivateKey {
        file
    }

    fn load_private_key_file(&self) -> eyre::Result<Self::PrivateKeyFile> {
        let private_key = std::fs::read_to_string(&self.private_key_file)?;
        serde_json::from_str(&private_key).map_err(Into::into)
    }

    fn load_genesis(&self) -> eyre::Result<Self::Genesis> {
        let genesis = std::fs::read_to_string(&self.genesis_file)?;
        serde_json::from_str(&genesis).map_err(Into::into)
    }

    async fn start(&self) -> eyre::Result<Handle> {
        let config = self.load_config()?;
        let span = tracing::error_span!("node", moniker = %config.moniker);
        let _enter = span.enter();

        let private_key_file = self.load_private_key_file()?;
        let private_key = self.load_private_key(private_key_file);
        let public_key = self.get_public_key(&private_key);
        let address = self.get_address(&public_key);
        let signing_provider = self.get_signing_provider(private_key);
        let ctx = MalakethContext::new();

        let genesis = self.load_genesis()?;
        let initial_validator_set = genesis.validator_set.clone();

        let codec = ProtobufCodec;

        let (mut channels, engine_handle) = malachitebft_app_channel::start_engine(
            ctx,
            self.clone(),
            config.clone(),
            codec, // WAL codec
            codec, // Network codec
            self.start_height,
            initial_validator_set,
        )
        .await?;

        let tx_event = channels.events.clone();

        let registry = SharedRegistry::global().with_moniker(&config.moniker);
        let metrics = DbMetrics::register(&registry);

        if config.metrics.enabled {
            tokio::spawn(metrics::serve(config.metrics.listen_addr));
        }

        let store = Store::open(self.get_home_dir().join("store.db"), metrics)?;
        let start_height = self.start_height.unwrap_or_default();
        let mut state = State::new(genesis, ctx, signing_provider, address, start_height, store);

        let malaketh_config = self.load_malaketh_config()?;

        let engine: Engine = {
            let engine_url = Url::parse(&malaketh_config.engine_authrpc_address)?;
            let jwt_path = PathBuf::from_str(&malaketh_config.jwt_token_path)?;

            let eth_url = Url::parse(&malaketh_config.execution_authrpc_address)?;
            Engine::new(
                EngineRPC::new(engine_url, jwt_path.as_path())?,
                EthereumRPC::new(eth_url)?,
            )
        };

        let app_handle = tokio::spawn(async move {
            if let Err(e) =
                crate::app::run(&mut state, &mut channels, engine, malaketh_config).await
            {
                tracing::error!(%e, "Application error");
            }
        });

        Ok(Handle {
            app: app_handle,
            engine: engine_handle,
            tx_event,
        })
    }

    async fn run(self) -> eyre::Result<()> {
        let handles = self.start().await?;
        handles.app.await.map_err(Into::into)
    }
}

impl CanMakeGenesis for App {
    fn make_genesis(&self, validators: Vec<(PublicKey, VotingPower)>) -> Self::Genesis {
        let validators = validators
            .into_iter()
            .map(|(pk, vp)| Validator::new(pk, vp));

        let validator_set = ValidatorSet::new(validators);

        Genesis { validator_set }
    }
}

impl CanGeneratePrivateKey for App {
    fn generate_private_key<R>(&self, rng: R) -> PrivateKey
    where
        R: RngCore + CryptoRng,
    {
        PrivateKey::generate(rng)
    }
}

impl CanMakePrivateKeyFile for App {
    fn make_private_key_file(&self, private_key: PrivateKey) -> Self::PrivateKeyFile {
        private_key
    }
}
