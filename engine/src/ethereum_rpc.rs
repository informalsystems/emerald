use color_eyre::eyre;
use reqwest::{header::CONTENT_TYPE, Client, Url};
use serde::de::DeserializeOwned;
use serde_json::json;
use std::time::Duration;
use tracing::debug;

use alloy_rpc_types_txpool::{TxpoolInspect, TxpoolStatus};

use crate::json_structures::*;

/// RPC client for Ethereum server.
pub struct EthereumRPC {
    client: Client,
    url: Url,
}

impl EthereumRPC {
    pub fn new(url: Url) -> eyre::Result<Self> {
        Ok(Self {
            client: Client::builder().build()?,
            url,
        })
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    pub async fn rpc_request<D: DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> eyre::Result<D> {
        let body = JsonRequestBody {
            jsonrpc: "2.0",
            method,
            params,
            id: json!(1),
        };
        let request = self
            .client
            .post(self.url.clone())
            .timeout(timeout)
            .header(CONTENT_TYPE, "application/json")
            .json(&body);
        let body: JsonResponseBody = request.send().await?.error_for_status()?.json().await?;

        debug!("response body: {:?}", body);

        match (body.result, body.error) {
            (result, None) => serde_json::from_value(result).map_err(Into::into),
            (_, Some(error)) => Err(eyre::eyre!(
                "Server Message: code: {}, message: {}",
                error.code,
                error.message,
            )),
        }
    }

    /// Get the eth1 chain id of the given endpoint.
    pub async fn get_chain_id(&self) -> eyre::Result<String> {
        self.rpc_request("eth_chainId", json!([]), Duration::from_secs(1))
            .await
    }

    pub async fn get_block_by_number(
        &self,
        block_number: &str,
    ) -> eyre::Result<Option<ExecutionBlock>> {
        let return_full_transaction_objects = false;
        let params = json!([block_number, return_full_transaction_objects]);
        self.rpc_request("eth_getBlockByNumber", params, Duration::from_secs(1))
            .await
    }

    pub async fn txpool_status(&self) -> eyre::Result<TxpoolStatus> {
        self.rpc_request("txpool_status", json!([]), Duration::from_secs(1))
            .await
    }

    pub async fn txpool_inspect(&self) -> eyre::Result<TxpoolInspect> {
        self.rpc_request("txpool_inspect", json!([]), Duration::from_secs(1))
            .await
    }
}
