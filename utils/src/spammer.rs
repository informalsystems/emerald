use crate::dex_templates::{RoundRobinSelector, TxTemplate};
use crate::make_signers;
use crate::tx::{make_signed_eip1559_tx, make_signed_eip4844_tx, make_signed_tx_from_template};
use alloy_network::eip2718::Encodable2718;
use alloy_primitives::Address;
use alloy_rpc_types_txpool::TxpoolStatus;
use alloy_signer_local::LocalSigner;
use color_eyre::eyre::{self, Result};
use core::fmt;
use k256::ecdsa::SigningKey;
use reqwest::header::CONTENT_TYPE;
use reqwest::{Client, Url};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::{self, sleep, Duration, Instant};
use tracing::{info, warn};

/// A transaction spammer that sends Ethereum transactions at a controlled rate.
/// Tracks and reports statistics on sent transactions.
pub struct Spammer {
    /// Spammer identifier.
    id: String,
    /// Client for Ethereum RPC node server.
    client: RpcClient,
    /// Ethereum transaction signer.
    signer: LocalSigner<SigningKey>,
    /// Maximum number of transactions to send (0 for no limit).
    max_num_txs: u64,
    /// Maximum number of seconds to run the spammer (0 for no limit).
    max_time: u64,
    /// Maximum number of transactions to send per second.
    max_rate: u64,
    /// Whether to send EIP-4844 blob transactions.
    blobs: bool,
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
            client: RpcClient::new(url),
            signer: signers[signer_index].clone(),
            max_num_txs,
            max_time,
            max_rate,
            blobs,
            templates: templates.map(|t| Arc::new(Mutex::new(RoundRobinSelector::new(t)))),
        })
    }

    pub async fn run(self) -> Result<()> {
        // Create channels for communication between spammer and tracker.
        let (result_sender, result_receiver) = mpsc::channel::<Result<u64>>(10000);
        let (report_sender, report_receiver) = mpsc::channel::<Instant>(1);
        let (finish_sender, finish_receiver) = mpsc::channel::<()>(1);

        let self_arc = Arc::new(self);

        // Spawn spammer.
        let spammer_handle = tokio::spawn({
            let self_arc = Arc::clone(&self_arc);
            async move {
                self_arc
                    .spammer(result_sender, report_sender, finish_sender)
                    .await
            }
        });

        // Spawn result tracker.
        let tracker_handle = tokio::spawn({
            let self_arc = Arc::clone(&self_arc);
            async move {
                self_arc
                    .tracker(result_receiver, report_receiver, finish_receiver)
                    .await
            }
        });

        let _ = tokio::join!(spammer_handle, tracker_handle);
        Ok(())
    }

    // Fetch from an Ethereum node the latest used nonce for the given address.
    async fn get_latest_nonce(&self, address: Address) -> Result<u64> {
        let response: String = self
            .client
            .rpc_request("eth_getTransactionCount", json!([address, "latest"]))
            .await?;
        // Convert hex string to integer.
        let hex_str = response.as_str().strip_prefix("0x").unwrap();
        Ok(u64::from_str_radix(hex_str, 16)?)
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
        println!("Spamming {address} starting from nonce={latest_nonce}");

        // Initialize nonce and counters.
        let mut nonce = latest_nonce;
        let start_time = Instant::now();
        let mut txs_sent_total = 0u64;
        let mut interval = time::interval(Duration::from_secs(1));
        loop {
            // Wait for next one-second tick.
            let _ = interval.tick().await;
            let mut txs_sent_in_interval = 0u64;
            let interval_start = Instant::now();

            // Send up to max_rate transactions per one-second interval.
            while txs_sent_in_interval < self.max_rate {
                // Check exit conditions before sending each transaction.
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
                        selector.next().clone()
                    };
                    make_signed_tx_from_template(&self.signer, &template, nonce).await?
                } else if self.blobs {
                    make_signed_eip4844_tx(&self.signer, nonce).await?
                } else {
                    make_signed_eip1559_tx(&self.signer, nonce).await?
                };
                let tx_bytes = signed_tx.encoded_2718();
                let tx_bytes_len = tx_bytes.len() as u64;

                // Send transaction to Ethereum RPC endpoint.
                let payload = hex::encode(tx_bytes);
                let result_unpacked: Result<String> = self
                    .client
                    .rpc_request("eth_sendRawTransaction", json!([payload]))
                    .await;

                // Log asynchronously without blocking the hot path
                match &result_unpacked {
                    Ok(tx_hash) => {
                        // Use tokio::spawn to offload logging to a separate task
                        let tx_hash = tx_hash.clone();
                        let nonce_val = nonce;
                        tokio::spawn(async move {
                            info!(nonce = nonce_val, tx_hash = %tx_hash, "Sent tx with hash.");
                        });
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        let nonce_val = nonce;
                        tokio::spawn(async move {
                            warn!(nonce = nonce_val, error = %error_msg, "Failed to send tx with nonce.");
                        });
                    }
                }

                let result = result_unpacked.map(|_| tx_bytes_len);

                // Report result and update counters.
                result_sender.send(result).await?;
                txs_sent_in_interval += 1;
                nonce += 1;
                txs_sent_total += 1;
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

                    let pool_status: TxpoolStatus = self.client.rpc_request("txpool_status", json!([])).await?;
                    println!("{stats_last_second}; {pool_status:?}");

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
        println!("Total: {stats_total}");
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

struct RpcClient {
    client: Client,
    url: Url,
}

impl RpcClient {
    pub fn new(url: Url) -> Self {
        let client = Client::new();
        Self { client, url }
    }

    pub async fn rpc_request<D: DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<D> {
        let body = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });
        let request = self
            .client
            .post(self.url.clone())
            .timeout(Duration::from_secs(1))
            .header(CONTENT_TYPE, "application/json")
            .json(&body);
        let body: JsonResponseBody = request.send().await?.error_for_status()?.json().await?;

        if let Some(JsonError { code, message }) = body.error {
            Err(eyre::eyre!("Server Error {}: {}", code, message))
        } else {
            serde_json::from_value(body.result).map_err(Into::into)
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonResponseBody {
    pub jsonrpc: String,
    #[serde(default)]
    pub error: Option<JsonError>,
    #[serde(default)]
    pub result: serde_json::Value,
    pub id: serde_json::Value,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonError {
    pub code: i64,
    pub message: String,
}
