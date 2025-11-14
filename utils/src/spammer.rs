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
use tracing::debug;

use crate::dex_templates::{RoundRobinSelector, TxTemplate};
use crate::make_signers;
use crate::tx::{
    make_signed_contract_call_tx, make_signed_eip1559_tx, make_signed_eip4844_tx,
    make_signed_tx_from_template,
};

struct ContractPayload {
    /// Contract address for contract call spamming.
    address: Address,
    /// Function signature for contract calls (e.g., "increment()").
    function_sig: String,
    /// Function arguments.
    args: Vec<String>,
}

/// A transaction spammer that sends Ethereum transactions at a controlled rate.
/// Tracks and reports statistics on sent transactions.
pub struct Spammer {
    /// Spammer identifier.
    id: String,
    /// Client for Ethereum RPC node server.
    client: RpcClient,
    /// Ethereum transaction signer.
    signer: PrivateKeySigner,
    /// Maximum number of transactions to send (0 for no limit).
    max_num_txs: u64,
    /// Maximum number of seconds to run the spammer (0 for no limit).
    max_time: u64,
    /// Maximum number of transactions to send per second.
    max_rate: u64,
    /// Whether to send EIP-4844 blob transactions.
    blobs: bool,
    /// Optional payload describing contract call spam parameters.
    contract_payload: Option<ContractPayload>,
    /// Optional transaction templates for round-robin spamming.
    /// Wrapped in Arc<Mutex<>> to allow shared mutable access across tasks.
    templates: Option<Arc<Mutex<RoundRobinSelector>>>,
}

impl Spammer {
    pub fn new(
        url: Url,
        signer_index: usize,
        max_num_txs: u64,
        max_time: u64,
        max_rate: u64,
        blobs: bool,
        templates: Option<Vec<TxTemplate>>,
    ) -> Result<Self> {
        let signers = make_signers();
        Ok(Self {
            id: signer_index.to_string(),
            client: RpcClient::new(url)?,
            signer: signers[signer_index].clone(),
            max_num_txs,
            max_time,
            max_rate,
            blobs,
            contract_payload: None,
            templates: templates.map(|t| Arc::new(Mutex::new(RoundRobinSelector::new(t)))),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_contract(
        url: Url,
        signer_index: usize,
        max_num_txs: u64,
        max_time: u64,
        max_rate: u64,
        contract: &Address,
        function: &str,
        args: &[String],
    ) -> Result<Self> {
        let signers = make_signers();
        let contract_payload = ContractPayload {
            address: *contract,
            function_sig: function.to_string(),
            args: args.to_vec(),
        };
        Ok(Self {
            id: signer_index.to_string(),
            client: RpcClient::new(url)?,
            signer: signers[signer_index].clone(),
            max_num_txs,
            max_time,
            max_rate,
            blobs: false, // Contract calls don't use blobs
            contract_payload: Some(contract_payload),
            templates: None,
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

    // Fetch from an Ethereum node the latest used nonce for the given address.
    async fn get_latest_nonce(&self, address: Address) -> Result<u64> {
        let response: String = self
            .client
            .rpc_request(
                "eth_getTransactionCount",
                vec![json!(address), json!("latest")],
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

    /// Generate and send transactions to the Ethereum node at a controlled rate.
    async fn spammer(
        &self,
        result_sender: Sender<Result<u64>>,
        report_sender: Sender<Instant>,
        finish_sender: Sender<()>,
    ) -> Result<()> {
        // Fetch latest nonce for the sender address.
        let address = self.signer.address();
        let latest_nonce = self.get_latest_nonce(address).await?;
        debug!("Spamming {address} starting from nonce={latest_nonce}");

        // Initialize nonce and counters.
        let mut nonce = latest_nonce;
        let start_time = Instant::now();
        let mut txs_sent_total = 0u64;
        let mut interval = time::interval(Duration::from_secs(1));

        // Channel for receiving nonce updates from background verification tasks
        let (nonce_update_sender, mut nonce_update_receiver) = mpsc::channel::<(u64, u64)>(1);

        loop {
            // Check for nonce updates from background tasks (non-blocking)
            while let Ok((expected_nonce, actual_nonce)) = nonce_update_receiver.try_recv() {
                if actual_nonce != expected_nonce {
                    eprintln!(
                        "⚠️  Nonce mismatch detected! Expected {expected_nonce}, but node has {actual_nonce}. Re-syncing..."
                    );
                    // Only update if the actual nonce is different from what we expected
                    nonce = actual_nonce;

                    // Calculate the correct template index based on the actual nonce
                    // This allows us to resume at the correct position in the template cycle
                    if let Some(ref templates) = self.templates {
                        let mut selector = templates.lock().unwrap();
                        let template_count = selector.template_count();

                        // Calculate how many transactions we've completed since latest_nonce
                        let nonce_offset = (actual_nonce - latest_nonce) as usize;
                        let correct_index = nonce_offset % template_count;

                        selector.set_index(correct_index);
                        eprintln!(
                            "Template index adjusted to {} (nonce offset: {}, template count: {})",
                            correct_index, nonce_offset, template_count
                        );
                    }
                }
            }

            // Wait for next one-second tick.
            let _ = interval.tick().await;
            let interval_start = Instant::now();

            // Prepare batch of transactions for this interval.
            let mut batch_entries = Vec::with_capacity(self.max_rate as usize);

            for _ in 0..self.max_rate {
                // Check exit conditions before creating each transaction.
                if (self.max_num_txs > 0 && txs_sent_total >= self.max_num_txs)
                    || (self.max_time > 0 && start_time.elapsed().as_secs() >= self.max_time)
                {
                    break;
                }

                // Create one transaction and sign it.
                let signed_tx = if let Some(ref templates) = self.templates {
                    // Use template-based transaction generation
                    let template = {
                        let mut selector = templates.lock().unwrap();
                        selector.next_template().clone()
                    };
                    make_signed_tx_from_template(&self.signer, &template, nonce).await?
                } else if let Some(ref payload) = self.contract_payload {
                    // Contract call transaction
                    make_signed_contract_call_tx(
                        &self.signer,
                        nonce,
                        payload.address,
                        &payload.function_sig,
                        payload.args.as_slice(),
                    )
                    .await?
                } else if self.blobs {
                    // Blob transaction
                    make_signed_eip4844_tx(&self.signer, nonce).await?
                } else {
                    // Regular transfer
                    make_signed_eip1559_tx(&self.signer, nonce).await?
                };
                let tx_bytes = signed_tx.encoded_2718();
                let tx_bytes_len = tx_bytes.len() as u64;

                // Add to batch.
                let payload = hex::encode(tx_bytes);
                batch_entries.push((vec![json!(payload)], tx_bytes_len));

                nonce += 1;
                txs_sent_total += 1;
            }

            // Send all transactions in a single batch RPC call.
            if !batch_entries.is_empty() {
                let params: Vec<_> = batch_entries
                    .iter()
                    .map(|(params, _)| params.clone())
                    .collect();

                let results = self
                    .client
                    .rpc_batch_request("eth_sendRawTransaction", params)
                    .await?;

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

                // Spawn background task to verify nonce (non-blocking)
                // This allows the spammer to continue immediately without waiting for the RPC call
                let nonce_sender = nonce_update_sender.clone();
                let client = self.client.clone();
                let check_address = address;
                let expected_nonce = nonce;
                tokio::spawn(async move {
                    if let Ok(actual_nonce) = client
                        .rpc_request::<String>(
                            "eth_getTransactionCount",
                            vec![json!(check_address), json!("pending")],
                        )
                        .await
                    {
                        if let Some(hex_str) = actual_nonce.strip_prefix("0x") {
                            if let Ok(nonce_value) = u64::from_str_radix(hex_str, 16) {
                                let _ = nonce_sender.try_send((expected_nonce, nonce_value));
                            }
                        }
                    }
                });
            }

            // Give time to the in-flight results to be received.
            sleep(Duration::from_millis(20)).await;

            // Signal tracker to report stats after this batch.
            report_sender.try_send(interval_start)?;

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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
            .request_timeout(Duration::from_secs(5))
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
