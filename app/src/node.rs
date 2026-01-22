//! The Application (or Node) definition. The Node trait implements the Consensus context and the
//! cryptographic library used for signing.

use core::str::FromStr;
use std::fs;
use std::path::PathBuf;

use alloy_genesis::Genesis as EvmGenesis;
use async_trait::async_trait;
use color_eyre::eyre;
use libp2p_identity::Keypair;
use malachitebft_app_channel::app::events::{RxEvent, TxEvent};
use malachitebft_app_channel::app::metrics::SharedRegistry;
use malachitebft_app_channel::app::node::{
    CanGeneratePrivateKey, CanMakeGenesis, CanMakePrivateKeyFile, EngineHandle, Node, NodeHandle,
};
use malachitebft_app_channel::app::types::core::VotingPower;
use malachitebft_app_channel::Channels;
use malachitebft_eth_cli::config::{Config, EmeraldConfig};
use malachitebft_eth_cli::metrics;
use malachitebft_eth_engine::engine::Engine;
use malachitebft_eth_engine::engine_rpc::EngineRPC;
use malachitebft_eth_engine::ethereum_rpc::EthereumRPC;
use malachitebft_eth_types::codec::proto::ProtobufCodec;
use malachitebft_eth_types::secp256k1::{K256Provider, PrivateKey, PublicKey};
use malachitebft_eth_types::{Address, EmeraldContext, Genesis, Height, Validator, ValidatorSet};
use rand::{CryptoRng, RngCore};
use tokio::task::JoinHandle;
use url::Url;

// Use the same types used for integration tests.
// A real application would use its own types and context instead.
use crate::metrics::Metrics;
use crate::state::{State, StateMetrics};
use crate::store::Store;

/// Main application struct implementing the consensus node functionality
#[derive(Clone)]
pub struct App {
    pub config: Config,
    pub home_dir: PathBuf,
    pub genesis_file: PathBuf,
    pub emerald_config_file: PathBuf,
    pub private_key_file: PathBuf,
    pub start_height: Option<Height>,
}

/// Components needed to run the application
pub struct AppRuntime {
    pub state: State,
    pub channels: Channels<EmeraldContext>,
    pub engine: Engine,
    pub emerald_config: EmeraldConfig,
    pub engine_handle: EngineHandle,
    pub tx_event: TxEvent<EmeraldContext>,
}

impl App {
    /// Build the application state and all necessary components.
    ///
    /// This function performs all the initialization and setup required to run
    /// the application, including loading configuration, initializing the
    /// consensus engine, and creating the state.
    ///
    /// Returns a [AppRuntime] struct containing the state and all components
    /// needed to run the app.
    pub async fn build_runtime(&self) -> eyre::Result<AppRuntime> {
        let config = self.load_config()?;
        let span = tracing::error_span!("node", moniker = %config.moniker);
        let _enter = span.enter();

        let private_key_file = self.load_private_key_file()?;
        let private_key = self.load_private_key(private_key_file);
        let public_key = self.get_public_key(&private_key);
        let address = self.get_address(&public_key);
        let signing_provider = self.get_signing_provider(private_key);
        let ctx = EmeraldContext::new();

        let genesis = self.load_genesis()?;
        let initial_validator_set = genesis.validator_set.clone();

        let codec = ProtobufCodec;

        let (channels, engine_handle) = malachitebft_app_channel::start_engine(
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
        let metrics = Metrics::register(&registry);

        if config.metrics.enabled {
            tokio::spawn(metrics::serve(config.metrics.listen_addr));
        }

        let store = Store::open(self.get_home_dir().join("store.db"), metrics.db.clone()).await?;
        let start_height = self.start_height.unwrap_or_default();

        // Load cumulative metrics from database for crash recovery
        let (txs_count, chain_bytes, elapsed_seconds) =
            store.load_cumulative_metrics().await?.unwrap_or_else(|| {
                tracing::info!("📊 No metrics found in database, starting with default values");
                (0, 0, 0)
            });

        let state_metrics = StateMetrics {
            txs_count,
            chain_bytes,
            elapsed_seconds,
            metrics,
        };

        let emerald_config = self.load_emerald_config()?;

        let engine: Engine = {
            let engine_url = Url::parse(&emerald_config.engine_authrpc_address)?;
            let jwt_path = PathBuf::from_str(&emerald_config.jwt_token_path)?;
            let eth_url = Url::parse(&emerald_config.execution_authrpc_address)?;
            Engine::new(
                EngineRPC::new(engine_url, jwt_path.as_path())?,
                EthereumRPC::new(eth_url)?,
            )
        };

        let min_block_time = emerald_config.min_block_time;
        let max_retain_blocks = emerald_config.max_retain_blocks;
        let prune_at_block_interval = emerald_config.prune_at_block_interval;

        assert!(
            prune_at_block_interval != 0,
            "prune block interval cannot be 0"
        );

        let eth_genesis_path = PathBuf::from_str(&emerald_config.eth_genesis_path)?;
        let eth_genesis: EvmGenesis = serde_json::from_str(&fs::read_to_string(eth_genesis_path)?)?;

        let evm_chain_config = eth_genesis.config;

        let state = State::new(
            genesis,
            ctx,
            signing_provider,
            address,
            start_height,
            store,
            state_metrics,
            max_retain_blocks,
            prune_at_block_interval,
            min_block_time,
            evm_chain_config,
        );

        Ok(AppRuntime {
            state,
            channels,
            engine,
            emerald_config,
            engine_handle,
            tx_event,
        })
    }

    fn load_emerald_config(&self) -> eyre::Result<EmeraldConfig> {
        let emerald_config_content =
            fs::read_to_string(&self.emerald_config_file).map_err(|e| {
                eyre::eyre!(
                    "Failed to read emerald config file `{}`: {e}",
                    self.emerald_config_file.display()
                )
            })?;
        let emerald_config = toml::from_str::<EmeraldConfig>(&emerald_config_content)
            .map_err(|e| eyre::eyre!("Failed to parse emerald config file: {e}"))?;
        Ok(emerald_config)
    }
}

pub struct Handle {
    pub app: JoinHandle<()>,
    pub engine: EngineHandle,
    pub tx_event: TxEvent<EmeraldContext>,
}

#[async_trait]
impl NodeHandle<EmeraldContext> for Handle {
    fn subscribe(&self) -> RxEvent<EmeraldContext> {
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
    type Context = EmeraldContext;
    type Config = Config;
    type Genesis = Genesis;
    type PrivateKeyFile = PrivateKey;
    type SigningProvider = K256Provider;
    type NodeHandle = Handle;

    fn get_home_dir(&self) -> PathBuf {
        self.home_dir.to_owned()
    }

    fn load_config(&self) -> eyre::Result<Self::Config> {
        Ok(self.config.clone())
    }

    fn get_signing_provider(&self, private_key: PrivateKey) -> Self::SigningProvider {
        K256Provider::new(private_key)
    }

    fn get_address(&self, pk: &PublicKey) -> Address {
        Address::from_public_key(pk)
    }

    fn get_public_key(&self, pk: &PrivateKey) -> PublicKey {
        pk.public_key()
    }

    fn get_keypair(&self, pk: PrivateKey) -> Keypair {
        use libp2p_identity::secp256k1::{Keypair as Secp256k1Keypair, SecretKey};

        let secret_bytes: [u8; 32] = pk.inner().to_bytes().into();
        let secret_key =
            SecretKey::try_from_bytes(secret_bytes).expect("failed to decode secp256k1 secret key");
        Secp256k1Keypair::from(secret_key).into()
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
        let AppRuntime {
            mut state,
            mut channels,
            engine,
            emerald_config,
            engine_handle,
            tx_event,
        } = self.build_runtime().await?;

        let app_handle = tokio::spawn(async move {
            if let Err(e) = crate::app::run(&mut state, &mut channels, engine, emerald_config).await
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
