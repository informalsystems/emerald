use std::time::{Duration, SystemTime, UNIX_EPOCH};

use alloy_rpc_types_txpool::TxpoolStatus;
use color_eyre::eyre::Result;
use reqwest::{Client, Url};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::signal;
use tokio::time::{interval, sleep};
use tracing::{error, info, warn};

/// A monitor that tracks when the mempool becomes empty with precise timestamps.
///
/// This monitor polls txpool_status at high frequency (configurable) and logs
/// the exact timestamp (in milliseconds) and mempool status using structured logging
pub struct MempoolMonitor {
    client: RpcClient,
    poll_interval_ms: u64,
}

impl MempoolMonitor {
    pub fn new(url: Url, poll_interval_ms: u64) -> Self {
        Self {
            client: RpcClient::new(url),
            poll_interval_ms,
        }
    }

    /// Run the monitor indefinitely, logging mempool events using structured logging.
    pub async fn run(self) -> Result<()> {
        // Set up signal handler in a separate task
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

        tokio::spawn(async move {
            match signal::ctrl_c().await {
                Ok(()) => {
                    eprintln!("\nReceived Ctrl+C signal");
                    let _ = shutdown_tx.send(()).await;
                }
                Err(e) => {
                    eprintln!("Error setting up signal handler: {e}");
                }
            }
        });

        let mut interval = interval(Duration::from_millis(self.poll_interval_ms));
        let mut last_was_empty: Option<bool> = None;

        println!("Starting mempool monitor. Press Ctrl+C to stop.");
        info!(
            "Starting mempool monitor with poll interval of {}ms",
            self.poll_interval_ms
        );

        let mut start_timestamp: u64 = 0;
        let mut end_timestamp: u64 = 0;

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    eprintln!("Shutting down monitor...");
                    info!("Received shutdown signal, stopping monitor...");

                    // Print statistics
                    if start_timestamp > 0 && end_timestamp > 0 {
                        let duration_ms = end_timestamp - start_timestamp;
                        eprintln!("\n=== Mempool Monitor Statistics ===");
                        eprintln!("Mempool was full for: {duration_ms} ms");
                        eprintln!("Start timestamp: {start_timestamp}");
                        eprintln!("End timestamp: {end_timestamp}");
                        info!(
                            duration_ms = duration_ms,
                            start_timestamp = start_timestamp,
                            end_timestamp = end_timestamp,
                            "Mempool full duration statistics"
                        );
                    } else {
                        warn!("No mempool full period detected during monitoring");
                        eprintln!("\n=== Mempool Monitor Statistics ===");
                        eprintln!("Mempool was never full during monitoring period");
                    }

                    break;
                }
                _ = interval.tick() => {
                    let timestamp_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;

                    match self
                        .client
                        .rpc_request::<TxpoolStatus>("txpool_status", json!([]))
                        .await
                    {
                        Ok(status) => {
                            let is_empty = status.pending == 0 && status.queued == 0;

                            // Only log on state transitions or first poll
                            let should_log = match last_was_empty {
                                None => true,                             // First poll, log initial state
                                Some(was_empty) => was_empty != is_empty, // Log on transition
                            };

                            if is_empty && should_log {
                                // Mempool just became empty - record end_timestamp
                                if start_timestamp > 0 && end_timestamp == 0 {
                                    end_timestamp = timestamp_ms;
                                    eprintln!("Mempool not empty at timestamp: {end_timestamp}");

                                }
                                info!(
                                    timestamp_ms = timestamp_ms,
                                    event = "MEMPOOL_EMPTY",
                                    pending = status.pending,
                                    queued = status.queued,
                                    "Mempool is now empty"
                                );

                            } else if !is_empty {
                                // Mempool just became filled - record start_timestamp
                                if start_timestamp == 0 {
                                    start_timestamp = timestamp_ms;
                                    eprintln!("Mempool empty at start timestamp: {start_timestamp}");
                                }
                                info!(
                                    timestamp_ms = timestamp_ms,
                                    event = "MEMPOOL_FILLED",
                                    pending = status.pending,
                                    queued = status.queued,
                                    "Mempool has transactions"
                                );
                            }

                            last_was_empty = Some(is_empty);
                        }
                        Err(e) => {
                            error!(
                                timestamp_ms = timestamp_ms,
                                error = %e,
                                "Error querying txpool_status"
                            );
                            // Small backoff on error
                            sleep(Duration::from_millis(5)).await;
                        }
                    }
                }
            }
        }

        Ok(())
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
            .timeout(Duration::from_millis(100)) // Fast timeout for high-frequency polling
            .header("Content-Type", "application/json")
            .json(&body);
        let body: JsonResponseBody = request.send().await?.error_for_status()?.json().await?;

        if let Some(error) = body.error {
            Err(color_eyre::eyre::eyre!(
                "Server Error {}: {}",
                error.code,
                error.message
            ))
        } else {
            serde_json::from_value(body.result).map_err(Into::into)
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonResponseBody {
    pub jsonrpc: String,
    #[serde(default)]
    pub error: Option<JsonError>,
    #[serde(default)]
    pub result: serde_json::Value,
    pub id: serde_json::Value,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct JsonError {
    pub code: i64,
    pub message: String,
}
