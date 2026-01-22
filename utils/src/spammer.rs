use core::fmt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use alloy_network::eip2718::Encodable2718;
use alloy_primitives::Address;
use alloy_rpc_types_txpool::TxpoolStatus;
use alloy_signer_local::PrivateKeySigner;
use color_eyre::eyre::{self, Result};
use jsonrpsee_core::client::ClientT;
use jsonrpsee_core::params::{ArrayParams, BatchRequestBuilder};
use jsonrpsee_http_client::{HttpClient, HttpClientBuilder};
use reqwest::Url;
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::{self, sleep, Duration, Instant};
use tracing::{debug, info};

use crate::tx::{make_signed_contract_call_tx, make_signed_eip1559_tx, make_signed_eip4844_tx};

/// Target pool size to maintain (in number of transactions).
const TARGET_POOL_SIZE: u64 = 30_000;

struct ContractPayload {
    /// Contract address for contract call spamming.
    address: Address,
    /// Function signature for contract calls (e.g., "increment()").
    function_sig: String,
    /// Function arguments.
    args: Vec<String>,
}

/// Configuration for the transaction spammer.
pub struct SpammerConfig {
    /// Maximum number of transactions to send (0 for no limit).
    pub max_num_txs: u64,
    /// Maximum number of seconds to run the spammer (0 for no limit).
    pub max_time: u64,
    /// Maximum number of transactions to send per second.
    pub max_rate: u64,
    /// Number of ms between sending batches of txs.
    pub batch_interval: u64,
    /// Whether to send EIP-4844 blob transactions.
    pub blobs: bool,
    /// Chain ID for the transactions.
    pub chain_id: u64,
    /// Signer indexes to use for transaction signing in rotation.
    pub signer_indexes: Vec<usize>,
}

/// A transaction spammer that sends Ethereum transactions at a controlled rate.
/// Tracks and reports statistics on sent transactions.
pub struct Spammer {
    /// Spammer identifier.
    id: String,
    /// Client for Ethereum RPC node server.
    client: RpcClient,
    /// Transaction signers.
    signers: Vec<PrivateKeySigner>,
    /// Current signer index for rotation.
    current_signer_index: Arc<Mutex<usize>>,
    /// Maximum number of transactions to send (0 for no limit).
    max_num_txs: u64,
    /// Maximum number of seconds to run the spammer (0 for no limit).
    max_time: u64,
    /// Maximum number of transactions to send per second.
    max_rate: u64,
    /// Number of ms between sending batches of txs (default: 200).
    batch_interval: u64,
    /// Whether to send EIP-4844 blob transactions.
    blobs: bool,
    /// Chain ID for the transactions.
    chain_id: u64,
    /// Optional payload describing contract call spam parameters.
    contract_payload: Option<ContractPayload>,
}

impl Spammer {
    pub fn new(url: Url, config: SpammerConfig) -> Result<Self> {
        // Create the signers needed for spammer
        let max_index = config.signer_indexes.iter().max().copied().unwrap_or(0);
        let all_signers = crate::genesis::make_signers_with_count((max_index + 1) as u64);

        let signers: Vec<PrivateKeySigner> = config
            .signer_indexes
            .iter()
            .map(|&idx| all_signers[idx].clone())
            .collect();

        let id = if config.signer_indexes.len() == 1 {
            config.signer_indexes[0].to_string()
        } else {
            format!(
                "{}-{}",
                config.signer_indexes[0],
                config.signer_indexes[config.signer_indexes.len() - 1]
            )
        };

        Ok(Self {
            id,
            client: RpcClient::new(url)?,
            signers,
            current_signer_index: Arc::new(Mutex::new(0)),
            max_num_txs: config.max_num_txs,
            max_time: config.max_time,
            max_rate: config.max_rate,
            batch_interval: config.batch_interval,
            blobs: config.blobs,
            chain_id: config.chain_id,
            contract_payload: None,
        })
    }

    pub fn new_contract(
        url: Url,
        config: SpammerConfig,
        contract: &Address,
        function: &str,
        args: &[String],
    ) -> Result<Self> {
        // Create the signers needed for spammer
        let max_index = config.signer_indexes.iter().max().copied().unwrap_or(0);
        let all_signers = crate::genesis::make_signers_with_count((max_index + 1) as u64);

        let signers: Vec<PrivateKeySigner> = config
            .signer_indexes
            .iter()
            .map(|&idx| all_signers[idx].clone())
            .collect();

        let id = if config.signer_indexes.len() == 1 {
            config.signer_indexes[0].to_string()
        } else {
            format!(
                "{}-{}",
                config.signer_indexes[0],
                config.signer_indexes[config.signer_indexes.len() - 1]
            )
        };

        let contract_payload = ContractPayload {
            address: *contract,
            function_sig: function.to_string(),
            args: args.to_vec(),
        };
        Ok(Self {
            id,
            client: RpcClient::new(url)?,
            signers,
            current_signer_index: Arc::new(Mutex::new(0)),
            max_num_txs: config.max_num_txs,
            max_time: config.max_time,
            max_rate: config.max_rate,
            batch_interval: config.batch_interval,
            blobs: false, // Contract calls don't use blobs
            contract_payload: Some(contract_payload),
            chain_id: config.chain_id,
        })
    }

    pub async fn run(self) -> Result<()> {
        // Create channels for communication between spammer and tracker.
        let (result_sender, result_receiver) = mpsc::channel::<Result<u64>>(10000);
        let (report_sender, report_receiver) = mpsc::channel::<Instant>(1);
        let (finish_sender, finish_receiver) = mpsc::channel::<()>(1);

        let self_arc = Arc::new(self);

        // Spammer future.
        let spammer_handle = {
            let self_arc = Arc::clone(&self_arc);
            async move {
                self_arc
                    .spammer(result_sender, report_sender, finish_sender)
                    .await
            }
        };

        // Result tracker future.
        let tracker_handle = {
            let self_arc = Arc::clone(&self_arc);
            async move {
                self_arc
                    .tracker(result_receiver, report_receiver, finish_receiver)
                    .await
            }
        };

        // Run spammer and result tracker concurrently.
        tokio::try_join!(spammer_handle, tracker_handle)?;

        Ok(())
    }

    // Fetch the next nonce to use for the given address.
    async fn get_next_nonce(&self, address: Address) -> Result<u64> {
        let response: String = self
            .client
            .rpc_request(
                "eth_getTransactionCount",
                vec![json!(address), json!("pending")],
            )
            .await?;
        // Convert hex string to integer.
        let hex_str = response.as_str().strip_prefix("0x").unwrap();
        Ok(u64::from_str_radix(hex_str, 16)?)
    }

    // Get current txpool status.
    async fn get_txpool_status(&self) -> Result<TxpoolStatus> {
        self.client.rpc_request("txpool_status", vec![]).await
    }

    // Get current number of pending and queued transactions in the pool.
    async fn get_mempool_count(&self) -> Result<u64> {
        let status = self.get_txpool_status().await?;
        Ok(status.pending + status.queued)
    }

    /// Generate and send transactions to the Ethereum node at a controlled rate.
    async fn spammer(
        &self,
        result_sender: Sender<Result<u64>>,
        report_sender: Sender<Instant>,
        finish_sender: Sender<()>,
    ) -> Result<()> {
        // Fetch latest nonces for all signer addresses
        let mut nonces: HashMap<Address, u64> = HashMap::new();
        for signer in &self.signers {
            let address = signer.address();
            let latest_nonce = self.get_next_nonce(address).await?;
            nonces.insert(address, latest_nonce);
            debug!("Signer {address} starting from nonce={latest_nonce}");
        }

        let txs_per_batch = self
            .max_rate
            .saturating_mul(self.batch_interval)
            .checked_div(1000)
            .unwrap_or(0);
        debug!(
            "Spamming with {} signer(s) at rate {}, sending {txs_per_batch} txs every {}ms",
            self.signers.len(),
            self.max_rate,
            self.batch_interval,
        );
        let start_time = Instant::now();
        let mut txs_sent_total = 0u64;
        let mut interval = time::interval(Duration::from_millis(self.batch_interval));

        loop {
            // Wait for next one-second tick.
            let _ = interval.tick().await;
            let interval_start = Instant::now();

            // Check if there are queued transactions (indicating nonce gaps)
            let txpool_status = self.get_txpool_status().await.ok();
            let queued_count = txpool_status.as_ref().map(|s| s.queued).unwrap_or(0);

            if queued_count > 0 {
                info!("Detected {} queued transactions, checking for nonce gaps", queued_count);

                // Check for nonce gaps for all signers (in batches to avoid rate limiting)
                const NONCE_CHECK_BATCH_SIZE: usize = 100;
                let mut nonce_results = Vec::new();

                for chunk in self.signers.chunks(NONCE_CHECK_BATCH_SIZE) {
                    let nonce_checks: Vec<_> = chunk
                        .iter()
                        .map(|signer| {
                            let current_nonce = nonces.get(&signer.address()).copied().unwrap_or(0);
                            async move {
                                let on_chain_nonce = self.get_next_nonce(signer.address()).await?;
                                Ok::<_, eyre::Error>((signer.address(), current_nonce, on_chain_nonce))
                            }
                        })
                        .collect();

                    let batch_results = futures::future::join_all(nonce_checks).await;
                    nonce_results.extend(batch_results);
                }

                let mut recovery_performed = false;
                for result in nonce_results {
                    let (address, current_nonce, on_chain_nonce) = result?;
                    let nonce_span = current_nonce.saturating_sub(on_chain_nonce);
                    if nonce_span > 0 {
                        info!("Signer {address}: current nonce={current_nonce}, on-chain nonce={on_chain_nonce}. Filling gap with {} txs", nonce_span);

                        // Find the signer by address
                        if let Some(signer) = self.signers.iter().find(|s| s.address() == address) {
                            // Build txs specifically for this signer to fill the gap
                            // Send nonces from on_chain_nonce to current_nonce - 1
                            let batch_entries = self.build_batch_entries_for_signer(signer, on_chain_nonce, current_nonce).await?;
                            if let Some(results) = self.send_raw_batch(&batch_entries).await? {
                                if results.len() != batch_entries.len() {
                                    return Err(eyre::eyre!(
                                        "Batch response count {} does not match request count {}",
                                        results.len(),
                                        batch_entries.len()
                                    ));
                                }

                                // Report individual results
                                for ((_, tx_bytes_len), result) in batch_entries.into_iter().zip(results) {
                                    let mapped_result = result.map(|_| tx_bytes_len);
                                    result_sender.send(mapped_result).await?;
                                }
                                recovery_performed = true;
                            } else {
                                debug!("Batch eth_sendRawTransaction timed out; skipping this tick");
                            }
                        }
                    }
                }

                // If recovery was performed, skip normal batch and wait for next interval
                if recovery_performed {
                    info!("Recovery performed, skipping normal batch for this interval");
                    let _ = report_sender.send(interval_start).await;
                    continue;
                }
            }

            // Get current pool size and calculate dynamic send rate
            let current_pool_size = self.get_mempool_count().await.unwrap_or(0);
            let space_available = TARGET_POOL_SIZE.saturating_sub(current_pool_size);
            let txs_to_send = if space_available < txs_per_batch {
                space_available
            } else {
                txs_per_batch
            };

            // Continue if there is no space available
            if txs_to_send == 0 {
                debug!("Mempool already full. Do not send more transactions.");
                let _ = report_sender.send(interval_start).await;
                continue;
            }

            // Limit the max number of transactions
            let tx_count = if self.max_num_txs > 0 {
                txs_to_send.min(self.max_num_txs.saturating_sub(txs_sent_total))
            } else {
                txs_to_send
            };

            // Prepare batch of transactions for this interval.
            let batch_entries = self.build_batch_entries(tx_count, &mut nonces).await?;
            let batch_size = batch_entries.len() as u64;

            debug!(
                "Pool: {current_pool_size}/{TARGET_POOL_SIZE}, sending {batch_size} txs (rate: {})",
                self.max_rate
            );

            // Send all transactions in a single batch RPC call.
            if !batch_entries.is_empty() {
                if let Some(results) = self.send_raw_batch(&batch_entries).await? {
                    if results.len() != batch_entries.len() {
                        return Err(eyre::eyre!(
                            "Batch response count {} does not match request count {}",
                            results.len(),
                            batch_entries.len()
                        ));
                    }

                    // Report individual results.
                    for ((_, tx_bytes_len), result) in batch_entries.into_iter().zip(results) {
                        let mapped_result = result.map(|_| tx_bytes_len);
                        result_sender.send(mapped_result).await?;
                    }

                    txs_sent_total += batch_size;
                } else {
                    debug!("Batch eth_sendRawTransaction timed out; skipping this tick");
                }
            }

            // Give time to the in-flight results to be received.
            sleep(Duration::from_millis(20)).await;

            // Signal tracker to report stats after this batch.
            let _ = report_sender.send(interval_start).await;

            // Check exit conditions after each tick.
            if (self.max_num_txs > 0 && txs_sent_total >= self.max_num_txs)
                || (self.max_time > 0 && start_time.elapsed().as_secs() >= self.max_time)
            {
                break;
            }
        }
        finish_sender.send(()).await?;
        Ok(())
    }

    async fn build_batch_entries(
        &self,
        tx_count: u64,
        nonces: &mut HashMap<Address, u64>,
    ) -> Result<Vec<(Vec<serde_json::Value>, u64)>> {
        let mut batch_entries = Vec::with_capacity(tx_count as usize);

        for _ in 0..tx_count {
            // Get the current signer by rotating through the list
            let signer_idx = {
                let mut idx = self.current_signer_index.lock().unwrap();
                let current = *idx;
                *idx = (*idx + 1) % self.signers.len();
                current
            };
            let signer = &self.signers[signer_idx];

            // Get and increment nonce for this signer
            let nonce = nonces.get(&signer.address()).copied().unwrap_or(0);
            nonces.insert(signer.address(), nonce + 1);

            debug!(
                "Adding tx to batch: signer={}, nonce={}",
                signer.address(),
                nonce
            );

            let signed_tx = if let Some(ref payload) = self.contract_payload {
                make_signed_contract_call_tx(
                    signer,
                    nonce,
                    payload.address,
                    &payload.function_sig,
                    payload.args.as_slice(),
                    self.chain_id,
                )
                .await?
            } else if self.blobs {
                make_signed_eip4844_tx(signer, nonce, self.chain_id).await?
            } else {
                make_signed_eip1559_tx(signer, nonce, self.chain_id).await?
            };

            let tx_bytes = signed_tx.encoded_2718();
            let tx_bytes_len = tx_bytes.len() as u64;
            let payload = hex::encode(tx_bytes);
            batch_entries.push((vec![json!(payload)], tx_bytes_len));
        }

        Ok(batch_entries)
    }

    async fn build_batch_entries_for_signer(
        &self,
        signer: &PrivateKeySigner,
        start_nonce: u64,
        end_nonce: u64,
    ) -> Result<Vec<(Vec<serde_json::Value>, u64)>> {
        let mut batch_entries = Vec::with_capacity((end_nonce - start_nonce) as usize);

        // Send txs for nonces from start_nonce to end_nonce - 1
        for nonce in start_nonce..end_nonce {
            debug!(
                "Adding recovery tx to batch: signer={}, nonce={}",
                signer.address(),
                nonce
            );

            let signed_tx = if let Some(ref payload) = self.contract_payload {
                make_signed_contract_call_tx(
                    signer,
                    nonce,
                    payload.address,
                    &payload.function_sig,
                    payload.args.as_slice(),
                    self.chain_id,
                )
                .await?
            } else if self.blobs {
                make_signed_eip4844_tx(signer, nonce, self.chain_id).await?
            } else {
                make_signed_eip1559_tx(signer, nonce, self.chain_id).await?
            };

            let tx_bytes = signed_tx.encoded_2718();
            let tx_bytes_len = tx_bytes.len() as u64;
            let payload = hex::encode(tx_bytes);
            batch_entries.push((vec![json!(payload)], tx_bytes_len));
        }

        Ok(batch_entries)
    }

    async fn send_raw_batch(
        &self,
        batch_entries: &[(Vec<serde_json::Value>, u64)],
    ) -> Result<Option<Vec<Result<String>>>> {
        let params: Vec<_> = batch_entries
            .iter()
            .map(|(params, _)| params.clone())
            .collect();

        match self
            .client
            .rpc_batch_request("eth_sendRawTransaction", params)
            .await
        {
            Ok(responses) => Ok(Some(responses)),
            Err(err) => {
                if let Some(jsonrpsee_core::client::Error::RequestTimeout) =
                    err.downcast_ref::<jsonrpsee_core::client::Error>()
                {
                    Ok(None)
                } else {
                    Err(err)
                }
            }
        }
    }

    // Track and report statistics on sent transactions.
    async fn tracker(
        &self,
        mut result_receiver: Receiver<Result<u64>>,
        mut report_receiver: Receiver<Instant>,
        mut finish_receiver: Receiver<()>,
    ) -> Result<()> {
        // Initialize counters
        let start_time = Instant::now();
        let mut stats_total = Stats::new(self.id.as_str(), start_time);
        let mut stats_last_second = Stats::new(self.id.as_str(), start_time);
        loop {
            tokio::select! {
                // Update counters
                Some(res) = result_receiver.recv() => {
                    match res {
                        Ok(tx_length) => stats_last_second.incr_ok(tx_length),
                        Err(error) => stats_last_second.incr_err(&error.to_string()),
                    }
                }
                // Report stats
                Some(interval_start) = report_receiver.recv() => {
                    // Wait what's missing to complete one second.
                    let elapsed = interval_start.elapsed();
                    if elapsed < Duration::from_secs(1) {
                        sleep(Duration::from_secs(1) - elapsed).await;
                    }

                    let pool_status = self.get_txpool_status().await?;
                    debug!("{stats_last_second}; {pool_status:?}");

                    // Update total, then reset last second stats
                    stats_total.add(&stats_last_second);
                    stats_last_second.reset();
                }
                // Stop tracking
                _ = finish_receiver.recv() => {
                    break;
                }
            }
        }
        debug!("Total: {stats_total}");
        Ok(())
    }
}

/// Statistics on sent transactions.
struct Stats {
    id: String,
    start_time: Instant,
    succeed: u64,
    bytes: u64,
    errors_counter: HashMap<String, u64>,
}

impl Stats {
    fn new(id: &str, start_time: Instant) -> Self {
        Self {
            id: id.to_string(),
            start_time,
            succeed: 0,
            bytes: 0,
            errors_counter: HashMap::new(),
        }
    }

    fn incr_ok(&mut self, tx_length: u64) {
        self.succeed += 1;
        self.bytes += tx_length;
    }

    fn incr_err(&mut self, error: &str) {
        self.errors_counter
            .entry(error.to_string())
            .and_modify(|count| *count += 1)
            .or_insert(1);
    }

    fn add(&mut self, other: &Self) {
        self.succeed += other.succeed;
        self.bytes += other.bytes;
        for (error, count) in &other.errors_counter {
            self.errors_counter
                .entry(error.to_string())
                .and_modify(|c| *c += count)
                .or_insert(*count);
        }
    }

    fn reset(&mut self) {
        self.succeed = 0;
        self.bytes = 0;
        self.errors_counter.clear();
    }
}

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let elapsed = self.start_time.elapsed().as_millis();
        let stats = format!(
            "[{}] elapsed {:.3}s: Sent {} txs ({} bytes)",
            self.id,
            elapsed as f64 / 1000f64,
            self.succeed,
            self.bytes
        );
        let stats_failed = if self.errors_counter.is_empty() {
            String::new()
        } else {
            let failed = self.errors_counter.values().copied().sum::<u64>();
            format!("; {} failed with {:?}", failed, self.errors_counter)
        };
        write!(f, "{stats}{stats_failed}")
    }
}

#[derive(Clone)]
struct RpcClient {
    client: HttpClient,
}

impl RpcClient {
    pub fn new(url: Url) -> Result<Self> {
        let client = HttpClientBuilder::default()
            .request_timeout(Duration::from_secs(1))
            .build(url)?;
        Ok(Self { client })
    }

    pub async fn rpc_request<D: DeserializeOwned>(
        &self,
        method: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<D> {
        let mut array_params = ArrayParams::new();
        for item in params {
            array_params.insert(item)?;
        }
        let result = self.client.request(method, array_params).await?;
        Ok(result)
    }

    pub async fn rpc_batch_request(
        &self,
        method: &str,
        batch_params: Vec<Vec<serde_json::Value>>,
    ) -> Result<Vec<Result<String>>> {
        let mut batch = BatchRequestBuilder::new();

        for params in &batch_params {
            let mut array_params = ArrayParams::new();
            for item in params {
                array_params.insert(item)?;
            }
            batch.insert(method, array_params)?;
        }

        let batch_response = self.client.batch_request(batch).await?;

        Ok(batch_response
            .into_iter()
            .map(|r| r.map_err(|e| eyre::eyre!("RPC error: {e:?}")))
            .collect())
    }
}
