use color_eyre::eyre;
use reqwest::{header::CONTENT_TYPE, Client, Url};
use serde::de::DeserializeOwned;
use serde_json::json;
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use alloy_rpc_types_engine::{
    ExecutionPayloadEnvelopeV3, ExecutionPayloadV3, ForkchoiceState, ForkchoiceUpdated,
    PayloadAttributes, PayloadId as AlloyPayloadId, PayloadStatus,
};

use malachitebft_eth_types::{BlockHash, B256};

use crate::auth::Auth;
use crate::json_structures::*;

pub const ENGINE_NEW_PAYLOAD_V1: &str = "engine_newPayloadV1";
pub const ENGINE_NEW_PAYLOAD_V2: &str = "engine_newPayloadV2";
pub const ENGINE_NEW_PAYLOAD_V3: &str = "engine_newPayloadV3";
pub const ENGINE_NEW_PAYLOAD_V4: &str = "engine_newPayloadV4";
pub const ENGINE_NEW_PAYLOAD_TIMEOUT: Duration = Duration::from_secs(8);

pub const ENGINE_GET_PAYLOAD_V1: &str = "engine_getPayloadV1";
pub const ENGINE_GET_PAYLOAD_V2: &str = "engine_getPayloadV2";
pub const ENGINE_GET_PAYLOAD_V3: &str = "engine_getPayloadV3";
pub const ENGINE_GET_PAYLOAD_V4: &str = "engine_getPayloadV4";
pub const ENGINE_GET_PAYLOAD_TIMEOUT: Duration = Duration::from_secs(2);

pub const ENGINE_FORKCHOICE_UPDATED_V1: &str = "engine_forkchoiceUpdatedV1";
pub const ENGINE_FORKCHOICE_UPDATED_V2: &str = "engine_forkchoiceUpdatedV2";
pub const ENGINE_FORKCHOICE_UPDATED_V3: &str = "engine_forkchoiceUpdatedV3";
pub const ENGINE_FORKCHOICE_UPDATED_TIMEOUT: Duration = Duration::from_secs(8);

pub const ENGINE_GET_PAYLOAD_BODIES_BY_HASH_V1: &str = "engine_getPayloadBodiesByHashV1";
pub const ENGINE_GET_PAYLOAD_BODIES_BY_RANGE_V1: &str = "engine_getPayloadBodiesByRangeV1";
pub const ENGINE_GET_PAYLOAD_BODIES_TIMEOUT: Duration = Duration::from_secs(10);

pub const ENGINE_EXCHANGE_CAPABILITIES: &str = "engine_exchangeCapabilities";
pub const ENGINE_EXCHANGE_CAPABILITIES_TIMEOUT: Duration = Duration::from_secs(1);

pub const ENGINE_GET_CLIENT_VERSION_V1: &str = "engine_getClientVersionV1";
pub const ENGINE_GET_CLIENT_VERSION_TIMEOUT: Duration = Duration::from_secs(1);

pub const ENGINE_GET_BLOBS_V1: &str = "engine_getBlobsV1";
pub const ENGINE_GET_BLOBS_TIMEOUT: Duration = Duration::from_secs(1);

// Engine API methods supported by this implementation
pub static NODE_CAPABILITIES: &[&str] = &[
    // ENGINE_NEW_PAYLOAD_V1,
    // ENGINE_NEW_PAYLOAD_V2,
    ENGINE_NEW_PAYLOAD_V3,
    // ENGINE_NEW_PAYLOAD_V4,
    // ENGINE_GET_PAYLOAD_V1,
    // ENGINE_GET_PAYLOAD_V2,
    ENGINE_GET_PAYLOAD_V3,
    // ENGINE_GET_PAYLOAD_V4,
    // ENGINE_FORKCHOICE_UPDATED_V1,
    // ENGINE_FORKCHOICE_UPDATED_V2,
    ENGINE_FORKCHOICE_UPDATED_V3,
    // ENGINE_GET_PAYLOAD_BODIES_BY_HASH_V1,
    // ENGINE_GET_PAYLOAD_BODIES_BY_RANGE_V1,
    // ENGINE_GET_CLIENT_VERSION_V1,
    // ENGINE_GET_BLOBS_V1,
];

#[derive(Clone, Copy, Debug)]
pub struct EngineCapabilities {
    pub new_payload_v1: bool,
    pub new_payload_v2: bool,
    pub new_payload_v3: bool,
    pub new_payload_v4: bool,
    pub forkchoice_updated_v1: bool,
    pub forkchoice_updated_v2: bool,
    pub forkchoice_updated_v3: bool,
    pub get_payload_bodies_by_hash_v1: bool,
    pub get_payload_bodies_by_range_v1: bool,
    pub get_payload_v1: bool,
    pub get_payload_v2: bool,
    pub get_payload_v3: bool,
    pub get_payload_v4: bool,
    pub get_client_version_v1: bool,
    pub get_blobs_v1: bool,
}

// RPC client for connecting to Engine RPC endpoint with JWT authentication.
pub struct EngineRPC {
    client: Client,
    url: Url,
    auth: Auth,
}

impl std::fmt::Display for EngineRPC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}

impl EngineRPC {
    pub fn new(url: Url, jwt_path: &Path) -> eyre::Result<Self> {
        Ok(Self {
            client: Client::builder().build()?,
            url,
            auth: Auth::new_from_path(jwt_path)
                .map_err(|error| eyre::eyre!("Failed to load configuration file: {error}"))?,
        })
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
        let token = self.auth.generate_token()?;
        let request = self
            .client
            .post(self.url.clone())
            .timeout(timeout)
            .header(CONTENT_TYPE, "application/json")
            .bearer_auth(token)
            .json(&body);
        let body: JsonResponseBody = request.send().await?.error_for_status()?.json().await?;

        if let Some(error) = body.error {
            Err(eyre::eyre!(
                "Server Message: code: {}, message: {}",
                error.code,
                error.message,
            ))
        } else {
            serde_json::from_value(body.result).map_err(Into::into)
        }
    }

    pub async fn exchange_capabilities(&self) -> eyre::Result<EngineCapabilities> {
        let capabilities: HashSet<String> = self
            .rpc_request(
                ENGINE_EXCHANGE_CAPABILITIES,
                json!([NODE_CAPABILITIES]),
                ENGINE_EXCHANGE_CAPABILITIES_TIMEOUT,
            )
            .await?;

        Ok(EngineCapabilities {
            new_payload_v1: capabilities.contains(ENGINE_NEW_PAYLOAD_V1),
            new_payload_v2: capabilities.contains(ENGINE_NEW_PAYLOAD_V2),
            new_payload_v3: capabilities.contains(ENGINE_NEW_PAYLOAD_V3),
            new_payload_v4: capabilities.contains(ENGINE_NEW_PAYLOAD_V4),
            forkchoice_updated_v1: capabilities.contains(ENGINE_FORKCHOICE_UPDATED_V1),
            forkchoice_updated_v2: capabilities.contains(ENGINE_FORKCHOICE_UPDATED_V2),
            forkchoice_updated_v3: capabilities.contains(ENGINE_FORKCHOICE_UPDATED_V3),
            get_payload_bodies_by_hash_v1: capabilities
                .contains(ENGINE_GET_PAYLOAD_BODIES_BY_HASH_V1),
            get_payload_bodies_by_range_v1: capabilities
                .contains(ENGINE_GET_PAYLOAD_BODIES_BY_RANGE_V1),
            get_payload_v1: capabilities.contains(ENGINE_GET_PAYLOAD_V1),
            get_payload_v2: capabilities.contains(ENGINE_GET_PAYLOAD_V2),
            get_payload_v3: capabilities.contains(ENGINE_GET_PAYLOAD_V3),
            get_payload_v4: capabilities.contains(ENGINE_GET_PAYLOAD_V4),
            get_client_version_v1: capabilities.contains(ENGINE_GET_CLIENT_VERSION_V1),
            get_blobs_v1: capabilities.contains(ENGINE_GET_BLOBS_V1),
        })
    }

    /// Notify that a fork choice has been updated, to set the head of the chain
    /// - head_block_hash: The block hash of the head of the chain
    /// - safe_block_hash: The block hash of the most recent "safe" block (can be same as head)
    /// - finalized_block_hash: The block hash of the highest finalized block (can be 0x0 for genesis)
    pub async fn forkchoice_updated(
        &self,
        head_block_hash: BlockHash,
        maybe_payload_attributes: Option<PayloadAttributes>,
    ) -> eyre::Result<ForkchoiceUpdated> {
        let forkchoice_state = ForkchoiceState {
            head_block_hash,
            safe_block_hash: head_block_hash,
            finalized_block_hash: head_block_hash,
        };
        self.rpc_request(
            ENGINE_FORKCHOICE_UPDATED_V3,
            json!([forkchoice_state, maybe_payload_attributes]),
            ENGINE_FORKCHOICE_UPDATED_TIMEOUT,
        )
        .await
    }

    pub async fn get_payload(
        &self,
        payload_id: AlloyPayloadId,
    ) -> eyre::Result<ExecutionPayloadV3> {
        let response: ExecutionPayloadEnvelopeV3 = self
            .rpc_request(
                ENGINE_GET_PAYLOAD_V4,
                json!([payload_id]),
                ENGINE_GET_PAYLOAD_TIMEOUT,
            )
            .await?;
        Ok(response.execution_payload)
    }

    pub async fn new_payload(
        &self,
        execution_payload: ExecutionPayloadV3,
        versioned_hashes: Vec<B256>,
        parent_block_hash: BlockHash,
        execution_requests: Vec<Vec<u8>>,
    ) -> eyre::Result<PayloadStatus> {
        let payload = JsonExecutionPayloadV3::from(execution_payload);
        let params = json!([
            payload,
            versioned_hashes,
            parent_block_hash,
            execution_requests
        ]);
        self.rpc_request(ENGINE_NEW_PAYLOAD_V4, params, ENGINE_NEW_PAYLOAD_TIMEOUT)
            .await
    }
}
