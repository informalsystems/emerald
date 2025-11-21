//! RPC helper utilities for testnet commands

use core::time::Duration;

use color_eyre::eyre::Context as _;
use color_eyre::Result;
use malachitebft_eth_engine::ethereum_rpc::EthereumRPC;
use reqwest::Url;
use serde_json::json;

/// Simple blocking RPC client wrapper
pub struct RpcClient {
    url: String,
}

impl RpcClient {
    pub fn new(port: u16) -> Self {
        Self {
            url: format!("http://127.0.0.1:{port}"),
        }
    }

    /// Get current block number
    pub fn get_block_number(&self) -> Result<u64> {
        // Suppress debug logs temporarily
        let _guard = tracing::subscriber::set_default(tracing::subscriber::NoSubscriber::default());

        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(async {
            let url = Url::parse(&self.url)?;
            let rpc = EthereumRPC::new(url)?;

            let result: String = rpc
                .rpc_request("eth_blockNumber", json!([]), Duration::from_secs(2))
                .await?;

            // Parse hex string (with 0x prefix)
            let block_number = u64::from_str_radix(result.trim_start_matches("0x"), 16)
                .context("Failed to parse block number")?;

            Ok(block_number)
        })
    }

    /// Get peer count
    pub fn get_peer_count(&self) -> Result<u64> {
        // Suppress debug logs temporarily
        let _guard = tracing::subscriber::set_default(tracing::subscriber::NoSubscriber::default());

        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(async {
            let url = Url::parse(&self.url)?;
            let rpc = EthereumRPC::new(url)?;

            let result: String = rpc
                .rpc_request("net_peerCount", json!([]), Duration::from_secs(2))
                .await?;

            // Parse hex string (with 0x prefix)
            let peer_count = u64::from_str_radix(result.trim_start_matches("0x"), 16)
                .context("Failed to parse peer count")?;

            Ok(peer_count)
        })
    }

    /// Get enode address
    pub fn get_enode(&self) -> Result<String> {
        // Suppress debug logs temporarily
        let _guard = tracing::subscriber::set_default(tracing::subscriber::NoSubscriber::default());

        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(async {
            let url = Url::parse(&self.url)?;
            let rpc = EthereumRPC::new(url)?;

            let result: serde_json::Value = rpc
                .rpc_request("admin_nodeInfo", json!([]), Duration::from_secs(2))
                .await?;

            let enode = result
                .get("enode")
                .and_then(|v| v.as_str())
                .ok_or_else(|| color_eyre::eyre::eyre!("No enode field in response"))?;

            Ok(enode.to_string())
        })
    }

    /// Add peer to node
    pub fn add_peer(&self, enode: &str) -> Result<()> {
        // Suppress debug logs temporarily
        let _guard = tracing::subscriber::set_default(tracing::subscriber::NoSubscriber::default());

        let runtime = tokio::runtime::Runtime::new()?;
        let enode = enode.to_string();

        runtime.block_on(async {
            let url = Url::parse(&self.url)?;
            let rpc = EthereumRPC::new(url)?;

            // Add as trusted peer
            let _ = rpc
                .rpc_request::<serde_json::Value>(
                    "admin_addTrustedPeer",
                    json!([enode.clone()]),
                    Duration::from_secs(2),
                )
                .await; // Ignore errors

            // Add as regular peer
            let _ = rpc
                .rpc_request::<serde_json::Value>(
                    "admin_addPeer",
                    json!([enode]),
                    Duration::from_secs(2),
                )
                .await; // Ignore errors

            Ok(())
        })
    }
}
