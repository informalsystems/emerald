//! Simplex Engine module that integrates simplex consensus with EVM execution.
//!
//! This module provides the main engine that drives the simplex consensus
//! using emerald's Engine API for EVM execution.

use core::num::NonZero;
use core::time::Duration;

use alloy_primitives::{Address, B256};
use commonware_broadcast::buffered;
use commonware_consensus::application::marshaled::Marshaled as ConsensusMarshaled;
use commonware_consensus::marshal::ingress::handler;
use commonware_consensus::marshal::{self};
use commonware_consensus::simplex::elector::Random;
use commonware_consensus::simplex::{self, Engine as Consensus};
use commonware_consensus::types::{Epoch, FixedEpocher, ViewDelta};
use commonware_consensus::{Reporter as ConsensusReporter, Reporters};
use commonware_cryptography::bls12381::primitives::group;
use commonware_cryptography::bls12381::primitives::sharing::Sharing;
use commonware_cryptography::bls12381::primitives::variant::MinSig;
use commonware_cryptography::certificate::{ConstantProvider, Scheme as CertificateScheme};
use commonware_cryptography::sha256::Digest;
use commonware_p2p::{Blocker, Receiver, Sender};
use commonware_parallel::Strategy;
use commonware_resolver::Resolver;
use commonware_runtime::buffer::PoolRef;
use commonware_runtime::{
    spawn_cell, Clock, ContextCell, Handle, Metrics, RayonPoolSpawner, Spawner, Storage,
};
use commonware_storage::archive::immutable;
use commonware_utils::ordered::Set;
use commonware_utils::{NZUsize, NZU16, NZU64};
use futures::channel::mpsc;
use futures::future::try_join_all;
use governor::clock::Clock as GClock;
use governor::Quota;
use malachitebft_eth_engine::engine::Engine as EmeraldEngine;
use rand::{CryptoRng, Rng};
use tracing::{error, info, warn};

use crate::application::Application;
use crate::block::Block;
use crate::consensus::{Activity, Finalization, PublicKey, Scheme, EPOCH, EPOCH_LENGTH, NAMESPACE};

/// Reporter type for simplex Engine.
type EngineReporter = Reporters<Activity, marshal::Mailbox<Scheme, Block>, NotarizationReporter>;

#[derive(Clone)]
struct NotarizationReporter {
    app: Application,
    mailbox: marshal::Mailbox<Scheme, Block>,
}

impl NotarizationReporter {
    fn new(app: Application, mailbox: marshal::Mailbox<Scheme, Block>) -> Self {
        Self { app, mailbox }
    }
}

impl ConsensusReporter for NotarizationReporter {
    type Activity = Activity;

    async fn report(&mut self, activity: Self::Activity) {
        match activity {
            Activity::Notarization(notarization) | Activity::Certification(notarization) => {
                let commitment = notarization.proposal.payload;
                let mut mailbox = self.mailbox.clone();
                let app = self.app.clone();
                tokio::spawn(async move {
                    let receiver = mailbox.subscribe(None, commitment).await;
                    match receiver.await {
                        Ok(block) => {
                            app.on_notarized(&block).await;
                        }
                        Err(_) => {
                            warn!(?commitment, "Notarized block subscription dropped");
                        }
                    }
                });
            }
            _ => {}
        }
    }
}

// Storage constants
const SYNCER_ACTIVITY_TIMEOUT_MULTIPLIER: u64 = 10;
const PRUNABLE_ITEMS_PER_SECTION: NonZero<u64> = NZU64!(4_096);
const IMMUTABLE_ITEMS_PER_SECTION: NonZero<u64> = NZU64!(262_144);
const FREEZER_TABLE_RESIZE_FREQUENCY: u8 = 4;
const FREEZER_TABLE_RESIZE_CHUNK_SIZE: u32 = 2u32.pow(16);
const FREEZER_JOURNAL_TARGET_SIZE: u64 = 1024 * 1024 * 1024;
const FREEZER_JOURNAL_COMPRESSION: Option<u8> = Some(3);
const REPLAY_BUFFER: NonZero<usize> = NZUsize!(8 * 1024 * 1024);
const WRITE_BUFFER: NonZero<usize> = NZUsize!(1024 * 1024);
const BUFFER_POOL_PAGE_SIZE: NonZero<u16> = NZU16!(4_096);
const BUFFER_POOL_CAPACITY: NonZero<usize> = NZUsize!(8_192);
const MAX_REPAIR: NonZero<usize> = NZUsize!(20);

/// Configuration for the simplex [Engine].
pub struct Config<B: Blocker<PublicKey = PublicKey>, S: Strategy> {
    pub blocker: B,
    pub partition_prefix: String,
    pub blocks_freezer_table_initial_size: u32,
    pub finalized_freezer_table_initial_size: u32,
    pub me: PublicKey,
    pub polynomial: Sharing<MinSig>,
    pub share: group::Share,
    pub participants: Set<PublicKey>,
    pub mailbox_size: usize,
    pub deque_size: usize,

    pub leader_timeout: Duration,
    pub notarization_timeout: Duration,
    pub nullify_retry: Duration,
    pub fetch_timeout: Duration,
    pub activity_timeout: ViewDelta,
    pub skip_timeout: ViewDelta,
    pub max_fetch_count: usize,
    pub max_fetch_size: usize,
    pub fetch_concurrent: usize,
    pub fetch_rate_per_peer: Quota,

    pub strategy: S,

    /// The emerald Engine API client.
    pub engine: EmeraldEngine,
    pub fee_recipient: Address,
    /// The genesis block hash from the execution layer.
    pub genesis_execution_hash: B256,
    pub min_block_time: Duration,
}

type Marshaled<E> = ConsensusMarshaled<E, Scheme, Application, Block, FixedEpocher>;

/// The simplex engine that drives the [Application].
#[allow(clippy::type_complexity)]
pub struct Engine<
    E: Clock + GClock + Rng + CryptoRng + Spawner + Storage + Metrics,
    B: Blocker<PublicKey = PublicKey>,
    S: Strategy,
> {
    context: ContextCell<E>,

    buffer: buffered::Engine<E, PublicKey, Block>,
    buffer_mailbox: buffered::Mailbox<PublicKey, Block>,
    marshal: marshal::Actor<
        E,
        Block,
        ConstantProvider<Scheme, Epoch>,
        immutable::Archive<E, Digest, Finalization>,
        immutable::Archive<E, Digest, Block>,
        FixedEpocher,
        S,
    >,
    marshaled: Marshaled<E>,

    consensus:
        Consensus<E, Scheme, Random, B, Digest, Marshaled<E>, Marshaled<E>, EngineReporter, S>,
}

impl<
        E: Clock
            + GClock
            + Rng
            + CryptoRng
            + Spawner
            + RayonPoolSpawner
            + Storage
            + Metrics
            + Send
            + Sync
            + 'static,
        B: Blocker<PublicKey = PublicKey>,
        S: Strategy,
    > Engine<E, B, S>
{
    /// Create a new simplex [Engine].
    pub async fn new(context: E, cfg: Config<B, S>) -> Self {
        // Create the buffer
        let (buffer, buffer_mailbox) = buffered::Engine::new(
            context.with_label("buffer"),
            buffered::Config {
                public_key: cfg.me,
                mailbox_size: cfg.mailbox_size,
                deque_size: cfg.deque_size,
                priority: true,
                codec_config: (),
            },
        );

        // Create the buffer pool
        let buffer_pool = PoolRef::new(BUFFER_POOL_PAGE_SIZE, BUFFER_POOL_CAPACITY);

        // Initialize finalizations archive
        let finalizations_by_height = immutable::Archive::init(
            context.with_label("finalizations_by_height"),
            immutable::Config {
                metadata_partition: format!(
                    "{}-finalizations-by-height-metadata",
                    cfg.partition_prefix
                ),
                freezer_table_partition: format!(
                    "{}-finalizations-by-height-freezer-table",
                    cfg.partition_prefix
                ),
                freezer_table_initial_size: cfg.finalized_freezer_table_initial_size,
                freezer_table_resize_frequency: FREEZER_TABLE_RESIZE_FREQUENCY,
                freezer_table_resize_chunk_size: FREEZER_TABLE_RESIZE_CHUNK_SIZE,
                freezer_key_partition: format!(
                    "{}-finalizations-by-height-freezer-key-journal",
                    cfg.partition_prefix
                ),
                freezer_key_buffer_pool: buffer_pool.clone(),
                freezer_key_write_buffer: WRITE_BUFFER,
                freezer_value_partition: format!(
                    "{}-finalizations-by-height-freezer-value-journal",
                    cfg.partition_prefix
                ),
                freezer_value_write_buffer: WRITE_BUFFER,
                freezer_value_target_size: FREEZER_JOURNAL_TARGET_SIZE,
                freezer_value_compression: FREEZER_JOURNAL_COMPRESSION,
                ordinal_partition: format!(
                    "{}-finalizations-by-height-ordinal",
                    cfg.partition_prefix
                ),
                ordinal_write_buffer: WRITE_BUFFER,
                items_per_section: IMMUTABLE_ITEMS_PER_SECTION,
                codec_config: Scheme::certificate_codec_config_unbounded(),
                replay_buffer: REPLAY_BUFFER,
            },
        )
        .await
        .expect("failed to initialize finalizations archive");
        info!("restored finalizations archive");

        // Initialize finalized blocks archive
        let finalized_blocks = immutable::Archive::init(
            context.with_label("finalized_blocks"),
            immutable::Config {
                metadata_partition: format!("{}-finalized_blocks-metadata", cfg.partition_prefix),
                freezer_table_partition: format!(
                    "{}-finalized_blocks-freezer-table",
                    cfg.partition_prefix
                ),
                freezer_table_initial_size: cfg.blocks_freezer_table_initial_size,
                freezer_table_resize_frequency: FREEZER_TABLE_RESIZE_FREQUENCY,
                freezer_table_resize_chunk_size: FREEZER_TABLE_RESIZE_CHUNK_SIZE,
                freezer_key_partition: format!(
                    "{}-finalized-blocks-freezer-key-journal",
                    cfg.partition_prefix
                ),
                freezer_key_buffer_pool: buffer_pool.clone(),
                freezer_key_write_buffer: WRITE_BUFFER,
                freezer_value_partition: format!(
                    "{}-finalized-blocks-freezer-value-journal",
                    cfg.partition_prefix
                ),
                freezer_value_write_buffer: WRITE_BUFFER,
                freezer_value_target_size: FREEZER_JOURNAL_TARGET_SIZE,
                freezer_value_compression: FREEZER_JOURNAL_COMPRESSION,
                ordinal_partition: format!("{}-finalized-blocks-ordinal", cfg.partition_prefix),
                ordinal_write_buffer: WRITE_BUFFER,
                items_per_section: IMMUTABLE_ITEMS_PER_SECTION,
                codec_config: (),
                replay_buffer: REPLAY_BUFFER,
            },
        )
        .await
        .expect("failed to initialize finalized blocks archive");
        info!("restored finalized blocks archive");

        // Create marshal
        let scheme = Scheme::signer(NAMESPACE, cfg.participants, cfg.polynomial, cfg.share)
            .expect("failed to create scheme");
        let provider = ConstantProvider::new(scheme.clone());
        let epocher = FixedEpocher::new(EPOCH_LENGTH);
        let (marshal, marshal_mailbox, _) = marshal::Actor::init(
            context.with_label("marshal"),
            finalizations_by_height,
            finalized_blocks,
            marshal::Config {
                provider,
                epocher: epocher.clone(),
                partition_prefix: cfg.partition_prefix.clone(),
                mailbox_size: cfg.mailbox_size,
                view_retention_timeout: ViewDelta::new(
                    cfg.activity_timeout
                        .get()
                        .saturating_mul(SYNCER_ACTIVITY_TIMEOUT_MULTIPLIER),
                ),
                prunable_items_per_section: PRUNABLE_ITEMS_PER_SECTION,
                replay_buffer: REPLAY_BUFFER,
                key_write_buffer: WRITE_BUFFER,
                value_write_buffer: WRITE_BUFFER,
                block_codec_config: (),
                max_repair: MAX_REPAIR,
                buffer_pool: buffer_pool.clone(),
                strategy: cfg.strategy.clone(),
            },
        )
        .await;

        // Create the application
        let app = Application::new(
            cfg.engine,
            cfg.fee_recipient,
            cfg.genesis_execution_hash,
            cfg.min_block_time,
        );
        let app_reporter = app.clone();
        let marshaled = Marshaled::new(
            context.with_label("marshaled"),
            app,
            marshal_mailbox.clone(),
            epocher,
        );

        // Create the reporter
        let notarization_reporter =
            NotarizationReporter::new(app_reporter, marshal_mailbox.clone());
        let reporter: EngineReporter = (marshal_mailbox, notarization_reporter).into();

        // Create the consensus engine
        let consensus = Consensus::new(
            context.with_label("consensus"),
            simplex::Config {
                epoch: EPOCH,
                scheme,
                automaton: marshaled.clone(),
                relay: marshaled.clone(),
                reporter,
                partition: format!("{}-consensus", cfg.partition_prefix),
                mailbox_size: cfg.mailbox_size,
                leader_timeout: cfg.leader_timeout,
                notarization_timeout: cfg.notarization_timeout,
                nullify_retry: cfg.nullify_retry,
                fetch_timeout: cfg.fetch_timeout,
                activity_timeout: cfg.activity_timeout,
                skip_timeout: cfg.skip_timeout,
                fetch_concurrent: cfg.fetch_concurrent,
                replay_buffer: REPLAY_BUFFER,
                write_buffer: WRITE_BUFFER,
                blocker: cfg.blocker,
                buffer_pool,
                elector: Random,
                strategy: cfg.strategy,
            },
        );

        Self {
            context: ContextCell::new(context),
            buffer,
            buffer_mailbox,
            marshal,
            marshaled,
            consensus,
        }
    }

    /// Start the simplex [Engine].
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        mut self,
        pending: (
            impl Sender<PublicKey = PublicKey>,
            impl Receiver<PublicKey = PublicKey>,
        ),
        recovered: (
            impl Sender<PublicKey = PublicKey>,
            impl Receiver<PublicKey = PublicKey>,
        ),
        resolver: (
            impl Sender<PublicKey = PublicKey>,
            impl Receiver<PublicKey = PublicKey>,
        ),
        broadcast: (
            impl Sender<PublicKey = PublicKey>,
            impl Receiver<PublicKey = PublicKey>,
        ),
        marshal: (
            mpsc::Receiver<handler::Message<Block>>,
            impl Resolver<Key = handler::Request<Block>, PublicKey = PublicKey>,
        ),
    ) -> Handle<()> {
        spawn_cell!(
            self.context,
            self.run(pending, recovered, resolver, broadcast, marshal)
                .await
        )
    }

    #[allow(clippy::too_many_arguments)]
    async fn run(
        self,
        pending: (
            impl Sender<PublicKey = PublicKey>,
            impl Receiver<PublicKey = PublicKey>,
        ),
        recovered: (
            impl Sender<PublicKey = PublicKey>,
            impl Receiver<PublicKey = PublicKey>,
        ),
        resolver: (
            impl Sender<PublicKey = PublicKey>,
            impl Receiver<PublicKey = PublicKey>,
        ),
        broadcast: (
            impl Sender<PublicKey = PublicKey>,
            impl Receiver<PublicKey = PublicKey>,
        ),
        marshal: (
            mpsc::Receiver<handler::Message<Block>>,
            impl Resolver<Key = handler::Request<Block>, PublicKey = PublicKey>,
        ),
    ) {
        // Start the buffer
        let buffer_handle = self.buffer.start(broadcast);

        // Start marshal
        let marshal_handle = self
            .marshal
            .start(self.marshaled, self.buffer_mailbox, marshal);

        // Start consensus
        let consensus_handle = self.consensus.start(pending, recovered, resolver);

        // Wait for any actor to finish
        if let Err(e) = try_join_all(vec![buffer_handle, marshal_handle, consensus_handle]).await {
            error!(?e, "Simplex engine failed");
        } else {
            warn!("Simplex engine stopped");
        }
    }
}
