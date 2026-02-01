use std::path::Path;

use alloy_eips::BlockNumberOrTag;
use alloy_primitives::{Bytes, B256};
use alloy_provider::ext::EngineApi;
use alloy_provider::network::Ethereum;
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_client::RpcClient;
use alloy_rpc_types_engine::{
    ExecutionPayloadV3, ForkchoiceState, ForkchoiceUpdated, JwtSecret, PayloadAttributes,
    PayloadId, PayloadStatus,
};
use alloy_rpc_types_eth::Block;
use alloy_transport_http::{AuthLayer, Http, HyperClient};
use url::Url;

#[derive(Clone, Copy, Debug)]
pub enum Fork {
    Osaka,
    Prague,
    Unsupported,
}

#[derive(Clone)]
pub struct EngineClient {
    engine: RootProvider<Ethereum>,
    eth: RootProvider<Ethereum>,
}

impl EngineClient {
    pub fn new(engine_url: Url, eth_url: Url, jwt_path: &Path) -> Result<Self, String> {
        let jwt_secret = JwtSecret::from_file(jwt_path).map_err(|e| e.to_string())?;
        let auth_client = HyperClient::new().layer(AuthLayer::new(jwt_secret));
        let http = Http::with_client(auth_client, engine_url);
        let rpc_client = RpcClient::new(http.clone(), http.guess_local());
        let engine = RootProvider::<Ethereum>::new(rpc_client);

        let eth_http = Http::with_client(HyperClient::new(), eth_url);
        let eth_client = RpcClient::new(eth_http.clone(), eth_http.guess_local());
        let eth = RootProvider::<Ethereum>::new(eth_client);

        Ok(Self { engine, eth })
    }

    // Fetch the genesis execution block via eth_getBlockByNumber(earliest).
    pub async fn get_genesis_block(&self) -> Result<Block, String> {
        let block = self
            .eth
            .get_block_by_number(BlockNumberOrTag::Earliest)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Genesis block not found in execution layer".to_string())?;

        Ok(block)
    }

    // Fetch the latest execution block via eth_getBlockByNumber(latest).
    pub async fn get_latest_block(&self) -> Result<Block, String> {
        let block = self
            .eth
            .get_block_by_number(BlockNumberOrTag::Latest)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Latest block not found in execution layer".to_string())?;

        Ok(block)
    }

    // Fetch a payload built after a forkchoice update (engine_getPayloadV5).
    pub async fn get_payload_v5(
        &self,
        payload_id: PayloadId,
    ) -> Result<ExecutionPayloadV3, String> {
        self.engine
            .get_payload_v5(payload_id)
            .await
            .map(|envelope| envelope.execution_payload)
            .map_err(|e| e.to_string())
    }

    // Fetch a payload built after a forkchoice update (engine_getPayloadV4).
    pub async fn get_payload_v4(
        &self,
        payload_id: PayloadId,
    ) -> Result<ExecutionPayloadV3, String> {
        self.engine
            .get_payload_v4(payload_id)
            .await
            .map(|envelope| envelope.envelope_inner.execution_payload)
            .map_err(|e| e.to_string())
    }

    // Submit a payload for validation and block import (engine_newPayloadV4).
    pub async fn new_payload_v4(
        &self,
        execution_payload: ExecutionPayloadV3,
        versioned_hashes: Vec<B256>,
        parent_beacon_block_root: B256,
    ) -> Result<PayloadStatus, String> {
        self.engine
            .new_payload_v4(
                execution_payload,
                versioned_hashes,
                parent_beacon_block_root,
                Vec::<Bytes>::new(),
            )
            .await
            .map_err(|e| e.to_string())
    }

    // Update head/safe/finalized forkchoice and optionally start payload build (engine_forkchoiceUpdatedV3).
    pub async fn fork_choice_updated_v3(
        &self,
        head_block_hash: B256,
        payload_attributes: Option<PayloadAttributes>,
    ) -> Result<ForkchoiceUpdated, String> {
        let forkchoice_state = ForkchoiceState {
            head_block_hash,
            safe_block_hash: head_block_hash,
            finalized_block_hash: head_block_hash,
        };

        self.engine
            .fork_choice_updated_v3(forkchoice_state, payload_attributes)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn get_latest_block_number(&self) -> Result<Option<u64>, String> {
        self.eth
            .get_block_number()
            .await
            .map(Some)
            .map_err(|e| e.to_string())
    }
}
