#![allow(clippy::result_large_err)]

use std::mem::size_of;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use malachitebft_app_channel::app::types::codec::Codec;
use malachitebft_app_channel::app::types::core::{CommitCertificate, Round};
use malachitebft_app_channel::app::types::ProposedValue;
use malachitebft_eth_types::codec::proto as codec;
use malachitebft_eth_types::codec::proto::ProtobufCodec;
use malachitebft_eth_types::{proto, Height, MalakethContext, Value, ValueId};
use malachitebft_proto::{Error as ProtoError, Protobuf};
use prost::Message;
use redb::ReadableTable;
use thiserror::Error;

mod keys;
use keys::{HeightKey, UndecidedValueKey};

use crate::metrics::DbMetrics;
use crate::store::keys::PendingValueKey;
use crate::streaming::ProposalParts;

#[derive(Clone, Debug)]
pub struct DecidedValue {
    pub value: Value,
    pub certificate: CommitCertificate<MalakethContext>,
}

fn decode_certificate(bytes: &[u8]) -> Result<CommitCertificate<MalakethContext>, ProtoError> {
    let proto = proto::CommitCertificate::decode(bytes)?;
    codec::decode_certificate(proto)
}

fn encode_certificate(
    certificate: &CommitCertificate<MalakethContext>,
) -> Result<Vec<u8>, ProtoError> {
    let proto = codec::encode_certificate(certificate)?;
    Ok(proto.encode_to_vec())
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("Database error: {0}")]
    Database(#[from] redb::DatabaseError),

    #[error("Storage error: {0}")]
    Storage(#[from] redb::StorageError),

    #[error("Table error: {0}")]
    Table(#[from] redb::TableError),

    #[error("Commit error: {0}")]
    Commit(#[from] redb::CommitError),

    #[error("Transaction error: {0}")]
    Transaction(#[from] redb::TransactionError),

    #[error("Failed to encode/decode Protobuf: {0}")]
    Protobuf(#[from] ProtoError),

    #[error("Failed to join on task: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),

    #[error("Failed to serialize/deserialize JSON: {0}")]
    Serialization(#[from] serde_json::Error),
}

const CERTIFICATES_TABLE: redb::TableDefinition<HeightKey, Vec<u8>> =
    redb::TableDefinition::new("certificates");

const DECIDED_VALUES_TABLE: redb::TableDefinition<HeightKey, Vec<u8>> =
    redb::TableDefinition::new("decided_values");

const UNDECIDED_PROPOSALS_TABLE: redb::TableDefinition<UndecidedValueKey, Vec<u8>> =
    redb::TableDefinition::new("undecided_values");

const DECIDED_BLOCK_DATA_TABLE: redb::TableDefinition<HeightKey, Vec<u8>> =
    redb::TableDefinition::new("decided_block_data");

const UNDECIDED_BLOCK_DATA_TABLE: redb::TableDefinition<UndecidedValueKey, Vec<u8>> =
    redb::TableDefinition::new("undecided_block_data");

const DECIDED_BLOCK_HEADERS_TABLE: redb::TableDefinition<HeightKey, Vec<u8>> =
    redb::TableDefinition::new("decided_block_headers");

const PERSISTENT_METRICS_TABLE: redb::TableDefinition<&str, u64> =
    redb::TableDefinition::new("persistent_metrics");

const PENDING_PROPOSAL_PARTS_TABLE: redb::TableDefinition<PendingValueKey, Vec<u8>> =
    redb::TableDefinition::new("pending_proposal_parts");

struct Db {
    db: redb::Database,
    metrics: DbMetrics,
}

impl Db {
    fn new(path: impl AsRef<Path>, metrics: DbMetrics) -> Result<Self, StoreError> {
        Ok(Self {
            db: redb::Database::create(path).map_err(StoreError::Database)?,
            metrics,
        })
    }

    fn get_decided_value(&self, height: Height) -> Result<Option<DecidedValue>, StoreError> {
        let start = Instant::now();
        let mut read_bytes = 0;

        let tx = self.db.begin_read()?;

        let value = {
            let table = tx.open_table(DECIDED_VALUES_TABLE)?;
            let value = table.get(&height)?;
            value.and_then(|value| {
                let bytes = value.value();
                read_bytes = bytes.len() as u64;
                Value::from_bytes(&bytes).ok()
            })
        };

        let certificate = {
            let table = tx.open_table(CERTIFICATES_TABLE)?;
            let value = table.get(&height)?;
            value.and_then(|value| {
                let bytes = value.value();
                read_bytes += bytes.len() as u64;
                decode_certificate(&bytes).ok()
            })
        };

        self.metrics.observe_read_time(start.elapsed());
        self.metrics.add_read_bytes(read_bytes);
        self.metrics.add_key_read_bytes(size_of::<Height>() as u64);

        let decided_value = value
            .zip(certificate)
            .map(|(value, certificate)| DecidedValue { value, certificate });

        Ok(decided_value)
    }

    fn insert_decided_value(
        &self,
        decided_value: DecidedValue,
        block_header_bytes: Bytes,
    ) -> Result<(), StoreError> {
        let start = Instant::now();
        let mut write_bytes = 0;

        let height = decided_value.certificate.height;
        let tx = self.db.begin_write()?;

        {
            let mut values = tx.open_table(DECIDED_VALUES_TABLE)?;
            let values_bytes = decided_value.value.to_bytes()?.to_vec();
            write_bytes += values_bytes.len() as u64;
            values.insert(height, values_bytes)?;
        }

        {
            let mut certificates = tx.open_table(CERTIFICATES_TABLE)?;
            let encoded_certificate = encode_certificate(&decided_value.certificate)?;
            write_bytes += encoded_certificate.len() as u64;
            certificates.insert(height, encoded_certificate)?;
        }

        {
            let mut headers = tx.open_table(DECIDED_BLOCK_HEADERS_TABLE)?;
            write_bytes += block_header_bytes.len() as u64;
            headers.insert(height, block_header_bytes.to_vec())?;
        }

        tx.commit()?;

        self.metrics.observe_write_time(start.elapsed());
        self.metrics.add_write_bytes(write_bytes);

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn get_undecided_proposal(
        &self,
        height: Height,
        round: Round,
        value_id: ValueId,
    ) -> Result<Option<ProposedValue<MalakethContext>>, StoreError> {
        let start = Instant::now();
        let mut read_bytes = 0;

        let tx = self.db.begin_read()?;
        let table = tx.open_table(UNDECIDED_PROPOSALS_TABLE)?;

        let value = if let Ok(Some(value)) = table.get(&(height, round, value_id)) {
            let bytes = value.value();
            read_bytes += bytes.len() as u64;

            let proposal = ProtobufCodec
                .decode(Bytes::from(bytes))
                .map_err(StoreError::Protobuf)?;

            Some(proposal)
        } else {
            None
        };

        self.metrics.observe_read_time(start.elapsed());
        self.metrics.add_read_bytes(read_bytes);
        self.metrics
            .add_key_read_bytes(size_of::<(Height, Round, ValueId)>() as u64);

        Ok(value)
    }

    fn get_undecided_proposals(
        &self,
        height: Height,
        round: Round,
    ) -> Result<Vec<ProposedValue<MalakethContext>>, StoreError> {
        let start = Instant::now();
        let mut read_bytes = 0;

        let tx = self.db.begin_read()?;
        let table = tx.open_table(UNDECIDED_PROPOSALS_TABLE)?;

        let mut proposals = Vec::new();
        for result in table.iter()? {
            let (key, value) = result?;
            let (h, r, _) = key.value();

            if h == height && r == round {
                let bytes = value.value();
                read_bytes += bytes.len() as u64;

                let proposal = ProtobufCodec
                    .decode(Bytes::from(bytes))
                    .map_err(StoreError::Protobuf)?;

                proposals.push(proposal);
            }
        }

        self.metrics.observe_read_time(start.elapsed());
        self.metrics.add_read_bytes(read_bytes);
        self.metrics.add_key_read_bytes(
            size_of::<(Height, Round, ValueId)>() as u64 * proposals.len() as u64,
        );

        Ok(proposals)
    }

    fn insert_undecided_proposal(
        &self,
        proposal: ProposedValue<MalakethContext>,
    ) -> Result<(), StoreError> {
        let start = Instant::now();

        let key = (proposal.height, proposal.round, proposal.value.id());
        let value = ProtobufCodec.encode(&proposal)?;

        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(UNDECIDED_PROPOSALS_TABLE)?;
            // Only insert if no value exists at this key
            if table.get(&key)?.is_none() {
                table.insert(key, value.to_vec())?;
            }
        }
        tx.commit()?;

        self.metrics.observe_write_time(start.elapsed());
        self.metrics.add_write_bytes(value.len() as u64);

        Ok(())
    }

    fn get_pending_proposal_parts(
        &self,
        height: Height,
        round: Round,
    ) -> Result<Vec<ProposalParts>, StoreError> {
        let start = Instant::now();
        let mut read_bytes = 0;

        let tx = self.db.begin_read()?;
        let table = tx.open_table(PENDING_PROPOSAL_PARTS_TABLE)?;

        let mut proposals = Vec::new();
        for result in table.iter()? {
            let (key, value) = result?;
            let (h, r, _) = key.value();

            if h == height && r == round {
                let bytes = value.value();
                read_bytes += bytes.len() as u64;

                let parts: ProposalParts = serde_json::from_slice(&bytes)?;

                proposals.push(parts);
            }
        }

        self.metrics.observe_read_time(start.elapsed());
        self.metrics.add_read_bytes(read_bytes);
        self.metrics.add_key_read_bytes(
            size_of::<(Height, Round, ValueId)>() as u64 * proposals.len() as u64,
        );

        Ok(proposals)
    }

    fn remove_pending_proposal_parts(&self, parts: ProposalParts) -> Result<(), StoreError> {
        let key = (
            parts.height,
            parts.round,
            Self::generate_value_id_from_parts(&parts),
        );
        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(PENDING_PROPOSAL_PARTS_TABLE)?;
            table.remove(key)?;
        }
        tx.commit()?;
        Ok(())
    }

    fn insert_pending_proposal_parts(&self, parts: ProposalParts) -> Result<(), StoreError> {
        let start = Instant::now();
        let key = (
            parts.height,
            parts.round,
            Self::generate_value_id_from_parts(&parts),
        );
        let value = serde_json::to_vec(&parts)?;

        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(PENDING_PROPOSAL_PARTS_TABLE)?;
            table.insert(key, value.clone())?;
        }
        tx.commit()?;

        self.metrics.observe_write_time(start.elapsed());
        self.metrics.add_write_bytes(value.len() as u64);

        Ok(())
    }

    // fn height_range<Table>(
    //     &self,
    //     table: &Table,
    //     range: impl RangeBounds<Height>,
    // ) -> Result<Vec<Height>, StoreError>
    // where
    //     Table: redb::ReadableTable<HeightKey, Vec<u8>>,
    // {
    //     Ok(table
    //         .range(range)?
    //         .flatten()
    //         .map(|(key, _)| key.value())
    //         .collect::<Vec<_>>())
    // }

    // Helper method to generate a unique ValueId from proposal parts
    pub fn generate_value_id_from_parts(parts: &ProposalParts) -> ValueId {
        use sha3::{Digest, Keccak256};

        let mut hasher = Keccak256::new();

        // Hash height, round, and proposer
        hasher.update(parts.height.as_u64().to_be_bytes());
        hasher.update(parts.round.as_i64().to_be_bytes());
        hasher.update(parts.proposer.into_inner());

        // Hash all the proposal parts content
        for part in &parts.parts {
            if let Some(data) = part.as_data() {
                hasher.update(data.bytes.as_ref());
            }
        }

        // In the generate_value_id_from_parts method:
        let hash = hasher.finalize();

        // Use first 8 bytes of hash to create ValueId
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&hash[..8]);
        ValueId::new(u64::from_be_bytes(bytes))
    }

    // All values except certificates can be retrieved from Reth (if the node has not been pruned)
    // But if we prune certificates, other nodes will not be able to catchup.
    fn prune(&self, retain_height: Height, prune_certificates: bool) -> Result<(), StoreError> {
        let start = Instant::now();

        let tx = self.db.begin_write().unwrap();

        {
            // Remove all undecided proposals with height < retain_height
            let mut undecided = tx.open_table(UNDECIDED_PROPOSALS_TABLE)?;
            undecided.retain(|k, _| k.0 >= retain_height)?;

            // Remove all undecided block data with height < retain_height
            let mut undecided_block_data = tx.open_table(UNDECIDED_BLOCK_DATA_TABLE)?;
            undecided_block_data.retain(|k, _| k.0 >= retain_height)?;

            // Remove all pending proposal parts with height < retain_height
            let mut pending = tx.open_table(PENDING_PROPOSAL_PARTS_TABLE)?;
            pending.retain(|k, _| k.0 >= retain_height)?;

            // Remove all decided values with height < retain_height
            let mut decided = tx.open_table(DECIDED_VALUES_TABLE)?;
            decided.retain(|k, _| k >= retain_height)?;

            // Remove all decided block data with height < retain_height
            let mut decided_block_data = tx.open_table(DECIDED_BLOCK_DATA_TABLE)?;
            decided_block_data.retain(|k, _| k >= retain_height)?;

            if prune_certificates {
                // We prune certificates only if pruning is set.
                let mut certificate_data = tx.open_table(CERTIFICATES_TABLE)?;
                certificate_data.retain(|k, _| k >= retain_height)?;
            }
        }

        tx.commit()?;

        self.metrics.observe_delete_time(start.elapsed());

        Ok(())
    }

    fn min_decided_value_height(&self) -> Option<Height> {
        let start = Instant::now();

        let tx = self.db.begin_read().unwrap();
        let table = tx.open_table(CERTIFICATES_TABLE).unwrap();
        let (key, value) = table.first().ok()??;

        self.metrics.observe_read_time(start.elapsed());
        self.metrics.add_read_bytes(value.value().len() as u64);
        self.metrics.add_key_read_bytes(size_of::<Height>() as u64);

        Some(key.value())
    }

    fn min_unpruned_decided_value_height(&self) -> Option<Height> {
        let start = Instant::now();

        let tx = self.db.begin_read().expect("failed to open db for reading");
        let table = tx
            .open_table(DECIDED_VALUES_TABLE)
            .expect("failed to open DECIDED_VALUES_TABLE");
        let (key, value) = table.first().ok()??;

        self.metrics.observe_read_time(start.elapsed());
        self.metrics.add_read_bytes(value.value().len() as u64);
        self.metrics.add_key_read_bytes(size_of::<Height>() as u64);

        Some(key.value())
    }

    fn max_decided_value_height(&self) -> Option<Height> {
        let tx = self
            .db
            .begin_read()
            .expect("failed for open db for reading");
        let table = tx
            .open_table(DECIDED_VALUES_TABLE)
            .expect("failed to open DECIDED_VALUES_TABLE");
        let (key, _) = table.last().ok()??;
        Some(key.value())
    }

    fn create_tables(&self) -> Result<(), StoreError> {
        let tx = self.db.begin_write()?;

        // Implicitly creates the tables if they do not exist yet
        let _ = tx.open_table(DECIDED_VALUES_TABLE)?;
        let _ = tx.open_table(CERTIFICATES_TABLE)?;
        let _ = tx.open_table(UNDECIDED_PROPOSALS_TABLE)?;
        let _ = tx.open_table(DECIDED_BLOCK_DATA_TABLE)?;
        let _ = tx.open_table(UNDECIDED_BLOCK_DATA_TABLE)?;
        let _ = tx.open_table(DECIDED_BLOCK_HEADERS_TABLE)?;
        let _ = tx.open_table(PERSISTENT_METRICS_TABLE)?;
        let _ = tx.open_table(PENDING_PROPOSAL_PARTS_TABLE)?;

        tx.commit()?;

        Ok(())
    }

    fn insert_cumulative_metrics(
        &self,
        txs_count: u64,
        chain_bytes: u64,
        elapsed_seconds: u64,
    ) -> Result<(), StoreError> {
        let start = Instant::now();
        let write_bytes = (size_of::<u64>() * 3) as u64;

        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(PERSISTENT_METRICS_TABLE)?;
            table.insert("txs_count", txs_count)?;
            table.insert("chain_bytes", chain_bytes)?;
            table.insert("elapsed_seconds", elapsed_seconds)?;
        }
        tx.commit()?;

        self.metrics.observe_write_time(start.elapsed());
        self.metrics.add_write_bytes(write_bytes);

        Ok(())
    }

    fn get_cumulative_metrics(&self) -> Result<Option<(u64, u64, u64)>, StoreError> {
        let start = Instant::now();
        let mut read_bytes = 0;

        let tx = self.db.begin_read()?;
        let table = tx.open_table(PERSISTENT_METRICS_TABLE)?;

        let txs_count = table.get("txs_count")?.map(|v| {
            read_bytes += size_of::<u64>() as u64;
            v.value()
        });

        let chain_bytes = table.get("chain_bytes")?.map(|v| {
            read_bytes += size_of::<u64>() as u64;
            v.value()
        });

        let elapsed_seconds = table.get("elapsed_seconds")?.map(|v| {
            read_bytes += size_of::<u64>() as u64;
            v.value()
        });

        self.metrics.observe_read_time(start.elapsed());
        self.metrics.add_read_bytes(read_bytes);
        self.metrics.add_key_read_bytes(
            ("txs_count".len() + "chain_bytes".len() + "elapsed_seconds".len()) as u64,
        );

        Ok(txs_count
            .zip(chain_bytes)
            .and_then(|(t, c)| elapsed_seconds.map(|e| (t, c, e))))
    }

    fn get_block_data(
        &self,
        height: Height,
        round: Round,
        value_id: ValueId,
    ) -> Result<Option<Bytes>, StoreError> {
        let start = Instant::now();

        let tx = self.db.begin_read()?;

        // Try undecided block data first
        let undecided_table = tx.open_table(UNDECIDED_BLOCK_DATA_TABLE)?;
        if let Some(data) = undecided_table.get(&(height, round, value_id))? {
            let bytes = data.value();
            let read_bytes = bytes.len() as u64;
            self.metrics.observe_read_time(start.elapsed());
            self.metrics.add_read_bytes(read_bytes);
            self.metrics.add_key_read_bytes(
                (size_of::<Height>() + size_of::<Round>() + size_of::<ValueId>()) as u64,
            );
            return Ok(Some(Bytes::copy_from_slice(&bytes)));
        }

        // Then try decided block data
        let decided_table = tx.open_table(DECIDED_BLOCK_DATA_TABLE)?;
        if let Some(data) = decided_table.get(&height)? {
            let bytes = data.value();
            let read_bytes = bytes.len() as u64;
            self.metrics.observe_read_time(start.elapsed());
            self.metrics.add_read_bytes(read_bytes);
            self.metrics.add_key_read_bytes(size_of::<Height>() as u64);
            return Ok(Some(Bytes::copy_from_slice(&bytes)));
        }

        self.metrics.observe_read_time(start.elapsed());
        Ok(None)
    }

    fn insert_undecided_block_data(
        &self,
        height: Height,
        round: Round,
        value_id: ValueId,
        data: Bytes,
    ) -> Result<(), StoreError> {
        let start = Instant::now();
        let write_bytes = data.len() as u64;

        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(UNDECIDED_BLOCK_DATA_TABLE)?;
            let key = (height, round, value_id);
            // Only insert if no value exists at this key
            if table.get(&key)?.is_none() {
                table.insert(key, data.to_vec())?;
            }
        }
        tx.commit()?;

        self.metrics.observe_write_time(start.elapsed());
        self.metrics.add_write_bytes(write_bytes);

        Ok(())
    }

    fn insert_decided_block_data(&self, height: Height, data: Bytes) -> Result<(), StoreError> {
        let start = Instant::now();
        let write_bytes = data.len() as u64;

        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(DECIDED_BLOCK_DATA_TABLE)?;
            // Only insert if no value exists at this key
            if table.get(&height)?.is_none() {
                table.insert(height, data.to_vec())?;
            }
        }
        tx.commit()?;

        self.metrics.observe_write_time(start.elapsed());
        self.metrics.add_write_bytes(write_bytes);

        Ok(())
    }

    fn get_certificate_and_header(
        &self,
        height: Height,
    ) -> Result<Option<(CommitCertificate<MalakethContext>, Bytes)>, StoreError> {
        let start = Instant::now();
        let mut read_bytes = 0;

        let tx = self.db.begin_read()?;

        let certificate = {
            let table = tx.open_table(CERTIFICATES_TABLE)?;
            table.get(&height)?.and_then(|v| {
                let bytes = v.value();
                read_bytes += bytes.len() as u64;
                decode_certificate(&bytes).ok()
            })
        };

        let header = {
            let table = tx.open_table(DECIDED_BLOCK_HEADERS_TABLE)?;
            table.get(&height)?.map(|v| {
                let bytes = v.value();
                read_bytes += bytes.len() as u64;
                Bytes::copy_from_slice(&bytes)
            })
        };

        self.metrics.observe_read_time(start.elapsed());
        self.metrics.add_read_bytes(read_bytes);
        self.metrics.add_key_read_bytes(size_of::<Height>() as u64);

        Ok(certificate.zip(header))
    }
}

#[derive(Clone)]
pub struct Store {
    db: Arc<Db>,
}

impl Store {
    /// Opens a new store at the given path with the provided metrics.
    /// Called by the application when initializing the store.
    pub async fn open(path: impl AsRef<Path>, metrics: DbMetrics) -> Result<Self, StoreError> {
        let path = path.as_ref().to_owned();

        tokio::task::spawn_blocking(move || {
            let db = Db::new(path, metrics)?;
            db.create_tables()?;
            Ok(Self { db: Arc::new(db) })
        })
        .await?
    }

    /// Returns the minimum height of decided values in the store.
    /// Called by the application to determine the earliest available height.
    pub async fn min_decided_value_height(&self) -> Option<Height> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.min_decided_value_height())
            .await
            .ok()
            .flatten()
    }

    pub async fn min_unpruned_decided_value_height(&self) -> Option<Height> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.min_unpruned_decided_value_height())
            .await
            .ok()
            .flatten()
    }

    pub async fn max_decided_value_height(&self) -> Option<Height> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.max_decided_value_height())
            .await
            .ok()
            .flatten()
    }

    /// Retrieves a decided value for the given height.
    /// Called by the application when a syncing peer is asking for a decided value.
    pub async fn get_decided_value(
        &self,
        height: Height,
    ) -> Result<Option<DecidedValue>, StoreError> {
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || db.get_decided_value(height)).await?
    }

    /// Stores a decided value with its certificate.
    /// Called by the application when it `commit`s a value decided by consensus.
    pub async fn store_decided_value(
        &self,
        certificate: &CommitCertificate<MalakethContext>,
        value: Value,
        block_header_bytes: Bytes,
    ) -> Result<(), StoreError> {
        let decided_value = DecidedValue {
            value,
            certificate: certificate.clone(),
        };

        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            db.insert_decided_value(decided_value, block_header_bytes)
        })
        .await?
    }

    /// Stores an undecided proposal.
    /// Called by the application when receiving new proposals from peers.
    pub async fn store_undecided_proposal(
        &self,
        value: ProposedValue<MalakethContext>,
    ) -> Result<(), StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.insert_undecided_proposal(value)).await?
    }

    /// Retrieves a specific undecided proposal by height, round, and value ID.
    /// Called by the application when consensus asks for a specific proposal to restream.
    pub async fn get_undecided_proposal(
        &self,
        height: Height,
        round: Round,
        value_id: ValueId,
    ) -> Result<Option<ProposedValue<MalakethContext>>, StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.get_undecided_proposal(height, round, value_id))
            .await?
    }

    /// Retrieves all undecided proposals for a given height and round.
    /// Called by the application when starting a new round and existing proposals need to be replayed.
    pub async fn get_undecided_proposals(
        &self,
        height: Height,
        round: Round,
    ) -> Result<Vec<ProposedValue<MalakethContext>>, StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.get_undecided_proposals(height, round)).await?
    }

    /// Stores a pending proposal parts.
    /// Called by the application when receiving new proposals from peers.
    pub async fn store_pending_proposal_parts(
        &self,
        value: ProposalParts,
    ) -> Result<(), StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.insert_pending_proposal_parts(value)).await?
    }

    /// Retrieves all pendingproposal parts for a given height and round.
    /// Called by the application when starting a new round and existing proposals need to be replayed.
    pub async fn get_pending_proposal_parts(
        &self,
        height: Height,
        round: Round,
    ) -> Result<Vec<ProposalParts>, StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.get_pending_proposal_parts(height, round)).await?
    }

    /// Removes a pending proposal parts.
    /// Called by the application when a proposal is no longer valid.
    pub async fn remove_pending_proposal_parts(
        &self,
        value: ProposalParts,
    ) -> Result<(), StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.remove_pending_proposal_parts(value)).await?
    }

    /// Prunes the store by removing all undecided proposals and decided values up to the retain height.
    /// Called by the application to clean up old data and free up space. This is done when a new value is committed.
    pub async fn prune(
        &self,
        retain_height: Height,
        prune_certificates: bool,
    ) -> Result<(), StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.prune(retain_height, prune_certificates)).await?
    }

    pub async fn get_block_data(
        &self,
        height: Height,
        round: Round,
        value_id: ValueId,
    ) -> Result<Option<Bytes>, StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.get_block_data(height, round, value_id)).await?
    }

    pub async fn store_undecided_block_data(
        &self,
        height: Height,
        round: Round,
        value_id: ValueId,
        data: Bytes,
    ) -> Result<(), StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            db.insert_undecided_block_data(height, round, value_id, data)
        })
        .await?
    }

    pub async fn store_decided_block_data(
        &self,
        height: Height,
        data: Bytes,
    ) -> Result<(), StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.insert_decided_block_data(height, data)).await?
    }

    pub async fn get_certificate_and_header(
        &self,
        height: Height,
    ) -> Result<Option<(CommitCertificate<MalakethContext>, Bytes)>, StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.get_certificate_and_header(height)).await?
    }

    pub async fn store_cumulative_metrics(
        &self,
        txs_count: u64,
        chain_bytes: u64,
        elapsed_seconds: u64,
    ) -> Result<(), StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            db.insert_cumulative_metrics(txs_count, chain_bytes, elapsed_seconds)
        })
        .await?
    }

    pub async fn load_cumulative_metrics(&self) -> Result<Option<(u64, u64, u64)>, StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.get_cumulative_metrics()).await?
    }
}
