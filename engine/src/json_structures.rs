use alloy_rpc_types::Withdrawal;
use alloy_rpc_types_engine::ExecutionPayloadV3;
use malachitebft_eth_types::{
    Address, BlockHash, BlockNumber, BlockTimestamp, Bloom, Bytes, B256, U256,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonRequestBody<'a> {
    pub jsonrpc: &'a str,
    pub method: &'a str,
    pub params: serde_json::Value,
    pub id: serde_json::Value,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonError {
    pub code: i64,
    pub message: String,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonPayloadAttributes {
    #[serde(with = "serde_utils::u64_hex_be")]
    pub timestamp: BlockTimestamp,
    pub prev_randao: B256,
    // #[serde(with = "serde_utils::address_hex")]
    pub suggested_fee_recipient: Address,
    pub withdrawals: Vec<JsonWithdrawal>,
    pub parent_beacon_block_root: BlockHash,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonWithdrawal {
    pub index: u64,
    pub validator_index: u64,
    pub address: Address,
    pub amount: u64,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonExecutionPayloadV3 {
    pub parent_hash: B256,
    // #[serde(with = "serde_utils::address_hex")]
    pub fee_recipient: Address,
    pub state_root: B256,
    pub receipts_root: B256,
    // #[serde(with = "serde_logs_bloom")]
    pub logs_bloom: Bloom,
    pub prev_randao: B256,
    #[serde(with = "serde_utils::u64_hex_be")]
    pub block_number: BlockNumber,
    #[serde(with = "serde_utils::u64_hex_be")]
    pub gas_limit: u64,
    #[serde(with = "serde_utils::u64_hex_be")]
    pub gas_used: u64,
    #[serde(with = "serde_utils::u64_hex_be")]
    pub timestamp: BlockTimestamp,
    pub extra_data: Bytes,
    #[serde(with = "serde_utils::u256_hex_be")]
    pub base_fee_per_gas: U256,
    pub block_hash: B256,
    // #[serde(with = "ssz_types::serde_utils::list_of_hex_var_list")]
    pub transactions: Vec<Bytes>,
    pub withdrawals: Vec<Withdrawal>,
    #[serde(with = "serde_utils::u64_hex_be")]
    pub blob_gas_used: u64,
    #[serde(with = "serde_utils::u64_hex_be")]
    pub excess_blob_gas: u64,
}

impl From<ExecutionPayloadV3> for JsonExecutionPayloadV3 {
    fn from(payload: ExecutionPayloadV3) -> Self {
        let v2 = payload.payload_inner;
        let v1 = v2.payload_inner;
        JsonExecutionPayloadV3 {
            parent_hash: v1.parent_hash,
            fee_recipient: v1.fee_recipient.into(),
            state_root: v1.state_root,
            receipts_root: v1.receipts_root,
            logs_bloom: v1.logs_bloom,
            prev_randao: v1.prev_randao,
            block_number: v1.block_number,
            gas_limit: v1.gas_limit,
            gas_used: v1.gas_used,
            timestamp: v1.timestamp,
            extra_data: v1.extra_data,
            base_fee_per_gas: v1.base_fee_per_gas,
            block_hash: v1.block_hash,
            transactions: v1.transactions,
            withdrawals: v2.withdrawals.into_iter().collect::<Vec<_>>(),
            blob_gas_used: payload.blob_gas_used,
            excess_blob_gas: payload.excess_blob_gas,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionBlock {
    #[serde(rename = "hash")]
    pub block_hash: BlockHash,

    #[serde(rename = "number", with = "serde_utils::u64_hex_be")]
    pub block_number: BlockNumber,

    pub parent_hash: BlockHash,

    #[serde(with = "serde_utils::u64_hex_be")]
    pub timestamp: BlockTimestamp,

    #[serde(rename = "mixHash")]
    pub prev_randao: B256,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPayloadBodyV1 {
    pub transactions: Vec<Bytes>,
    pub withdrawals: Option<Vec<Withdrawal>>,
}
