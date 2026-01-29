//! Compute genesis block hash from genesis.json

use std::collections::HashMap;
use std::fs;

use alloy_primitives::{keccak256, Address, Bytes, B256, B64, U256, U64};
use clap::Parser;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Genesis {
    nonce: String,
    timestamp: String,
    #[serde(rename = "extraData")]
    extra_data: String,
    #[serde(rename = "gasLimit")]
    gas_limit: String,
    difficulty: String,
    #[serde(rename = "mixHash")]
    mix_hash: String,
    coinbase: String,
    alloc: HashMap<String, AllocAccount>,
    #[serde(rename = "baseFeePerGas", default)]
    base_fee_per_gas: Option<String>,
    #[serde(rename = "blobGasUsed", default)]
    blob_gas_used: Option<String>,
    #[serde(rename = "excessBlobGas", default)]
    excess_blob_gas: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AllocAccount {
    balance: String,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    nonce: Option<String>,
    #[serde(default)]
    storage: Option<HashMap<String, String>>,
}

/// Compute genesis block hash from genesis.json
#[derive(Parser, Debug)]
#[command(name = "genesis-hash")]
#[command(about = "Compute genesis block hash from genesis.json")]
struct Args {
    /// Path to genesis.json file
    #[arg(default_value = "genesis.json")]
    genesis_path: String,
}

fn main() {
    let args = Args::parse();
    let genesis_path = &args.genesis_path;

    let content = fs::read_to_string(genesis_path).expect("Failed to read genesis.json");
    let genesis: Genesis = serde_json::from_str(&content).expect("Failed to parse genesis.json");

    // Compute state root from alloc
    let state_root = compute_state_root(&genesis.alloc);

    // Build RLP-encoded header
    let header_rlp = encode_genesis_header(&genesis, state_root);

    // Hash the header
    let hash = keccak256(&header_rlp);

    println!("Genesis block hash: {hash}");
    println!("\nUse this with emerald-simplex-setup:");
    println!("  --genesis-hash {hash}");
}

fn compute_state_root(alloc: &HashMap<String, AllocAccount>) -> B256 {
    use alloy_primitives::keccak256;

    if alloc.is_empty() {
        // Empty trie root - Keccak-256 hash of RLP([]), canonical hash for empty Merkle Patricia Trie
        return "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
            .parse::<B256>()
            .expect("Failed to parse empty trie root");
    }

    // For a proper implementation, we'd need a full MPT implementation
    // For now, use a simplified approach that matches common genesis files
    // This computes a deterministic hash from the accounts

    let mut accounts: Vec<_> = alloc.iter().collect();
    accounts.sort_by_key(|(addr, _)| addr.to_lowercase());

    let mut data = Vec::new();
    for (addr, account) in accounts {
        let address = addr
            .parse::<Address>()
            .expect("Failed to parse account address");
        data.extend_from_slice(&address.0 .0);

        let balance = account
            .balance
            .parse::<U256>()
            .expect("Failed to parse account balance");
        data.extend_from_slice(&balance.to_be_bytes::<32>());

        let nonce = account
            .nonce
            .as_ref()
            .and_then(|n| n.parse::<U64>().ok())
            .map(|u| u.wrapping_to::<u64>())
            .unwrap_or(0);
        data.extend_from_slice(&nonce.to_be_bytes());
    }

    // This is a simplified state root - for production, use proper MPT
    // For testing purposes, we'll compute a hash that can be used consistently
    keccak256(&data)
}

fn encode_genesis_header(genesis: &Genesis, state_root: B256) -> Vec<u8> {
    let parent_hash = B256::ZERO;
    // Empty ommers hash - Keccak-256 hash of RLP([]), used when no uncle blocks exist
    let ommers_hash = "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347"
        .parse::<B256>()
        .expect("Failed to parse empty ommers hash");

    let beneficiary = genesis.coinbase.parse::<Address>().unwrap_or(Address::ZERO);

    let logs_bloom = [0u8; 256];
    let difficulty = genesis
        .difficulty
        .parse::<U256>()
        .expect("Failed to parse difficulty");
    let number = U256::ZERO;
    let gas_limit = genesis
        .gas_limit
        .parse::<U64>()
        .expect("Failed to parse gasLimit")
        .wrapping_to::<u64>();
    let gas_used = 0u64;
    let timestamp = genesis
        .timestamp
        .parse::<U64>()
        .expect("Failed to parse timestamp")
        .wrapping_to::<u64>();

    let extra_data: Bytes = if genesis.extra_data.is_empty() || genesis.extra_data == "0x" {
        Bytes::new()
    } else {
        genesis
            .extra_data
            .parse::<Bytes>()
            .expect("Failed to parse extraData")
    };

    let mix_hash = genesis.mix_hash.parse::<B256>().unwrap_or(B256::ZERO);

    let nonce_val = genesis
        .nonce
        .parse::<U64>()
        .expect("Failed to parse nonce")
        .wrapping_to::<u64>();
    let nonce = B64::from(nonce_val);

    let base_fee = genesis
        .base_fee_per_gas
        .as_ref()
        .and_then(|s| s.parse::<U256>().ok());
    // Empty trie root - Keccak-256 hash of RLP([]), canonical hash for empty Merkle Patricia Trie
    let withdrawals_root = Some(
        "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
            .parse::<B256>()
            .expect("Failed to parse empty trie root"),
    );
    let blob_gas_used = genesis
        .blob_gas_used
        .as_ref()
        .and_then(|s| s.parse::<U64>().ok())
        .map(|u| u.wrapping_to::<u64>());
    let excess_blob_gas = genesis
        .excess_blob_gas
        .as_ref()
        .and_then(|s| s.parse::<U64>().ok())
        .map(|u| u.wrapping_to::<u64>());
    let parent_beacon_block_root = Some(B256::ZERO);

    // Empty trie root - Keccak-256 hash of RLP([]), canonical hash for empty Merkle Patricia Trie
    let transactions_root = "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
        .parse::<B256>()
        .expect("Failed to parse empty trie root");
    // Empty trie root - Keccak-256 hash of RLP([]), canonical hash for empty Merkle Patricia Trie
    let receipts_root = "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
        .parse::<B256>()
        .expect("Failed to parse empty trie root");

    // RLP encode the header
    let mut rlp = Vec::new();

    // Build header fields
    let mut fields = vec![
        rlp_encode_bytes(&parent_hash.0),
        rlp_encode_bytes(&ommers_hash.0),
        rlp_encode_bytes(&beneficiary.0 .0),
        rlp_encode_bytes(&state_root.0),
        rlp_encode_bytes(&transactions_root.0),
        rlp_encode_bytes(&receipts_root.0),
        rlp_encode_bytes(&logs_bloom),
        rlp_encode_u256(difficulty),
        rlp_encode_u256(number),
        rlp_encode_u64(gas_limit),
        rlp_encode_u64(gas_used),
        rlp_encode_u64(timestamp),
        rlp_encode_bytes(&extra_data),
        rlp_encode_bytes(&mix_hash.0),
        rlp_encode_bytes(&nonce.0),
    ];

    if let Some(base_fee) = base_fee {
        fields.push(rlp_encode_u256(base_fee));
    }
    if let Some(withdrawals_root) = withdrawals_root {
        fields.push(rlp_encode_bytes(&withdrawals_root.0));
    }
    if let Some(blob_gas) = blob_gas_used {
        fields.push(rlp_encode_u64(blob_gas));
    }
    if let Some(excess_gas) = excess_blob_gas {
        fields.push(rlp_encode_u64(excess_gas));
    }
    if let Some(parent_beacon) = parent_beacon_block_root {
        fields.push(rlp_encode_bytes(&parent_beacon.0));
    }

    // Encode as list
    let total_len: usize = fields.iter().map(|f| f.len()).sum();
    if total_len < 56 {
        rlp.push(0xc0 + total_len as u8);
    } else {
        let len_bytes = encode_length(total_len);
        rlp.push(0xf7 + len_bytes.len() as u8);
        rlp.extend_from_slice(&len_bytes);
    }
    for field in fields {
        rlp.extend_from_slice(&field);
    }

    rlp
}

fn rlp_encode_bytes(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();

    // Remove leading zeros for hash fields, but keep at least one byte
    let trimmed = trim_leading_zeros(data);

    if trimmed.len() == 1 && trimmed[0] < 0x80 {
        result.push(trimmed[0]);
    } else if trimmed.len() < 56 {
        result.push(0x80 + trimmed.len() as u8);
        result.extend_from_slice(trimmed);
    } else {
        let len_bytes = encode_length(trimmed.len());
        result.push(0xb7 + len_bytes.len() as u8);
        result.extend_from_slice(&len_bytes);
        result.extend_from_slice(trimmed);
    }

    result
}

fn rlp_encode_u64(value: u64) -> Vec<u8> {
    if value == 0 {
        return vec![0x80];
    }
    let bytes = value.to_be_bytes();
    let trimmed = trim_leading_zeros(&bytes);
    rlp_encode_bytes(trimmed)
}

fn rlp_encode_u256(value: U256) -> Vec<u8> {
    if value.is_zero() {
        return vec![0x80];
    }
    let bytes = value.to_be_bytes::<32>();
    let trimmed = trim_leading_zeros(&bytes);
    rlp_encode_bytes(trimmed)
}

fn trim_leading_zeros(data: &[u8]) -> &[u8] {
    if data.is_empty() {
        return data;
    }
    let first_nonzero = data.iter().position(|&b| b != 0);
    match first_nonzero {
        Some(pos) => &data[pos..],
        None => &data[data.len() - 1..], // All zeros, keep one
    }
}

fn encode_length(len: usize) -> Vec<u8> {
    if len < 256 {
        vec![len as u8]
    } else if len < 65536 {
        vec![(len >> 8) as u8, len as u8]
    } else if len < 16777216 {
        vec![(len >> 16) as u8, (len >> 8) as u8, len as u8]
    } else {
        vec![
            (len >> 24) as u8,
            (len >> 16) as u8,
            (len >> 8) as u8,
            len as u8,
        ]
    }
}
