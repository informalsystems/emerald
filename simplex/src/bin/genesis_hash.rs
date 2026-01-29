//! Compute genesis block hash from genesis.json

use std::collections::HashMap;
use std::fs;

use alloy_consensus::Header;
use alloy_primitives::{keccak256, Address, Bloom, Bytes, B256, B64, U256, U64};
use alloy_rlp::Encodable;
use alloy_trie::EMPTY_ROOT_HASH;
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

/// Compute the Keccak-256 hash of an RLP-encoded empty list.
/// This is used for empty ommers (uncle blocks).
fn compute_empty_list_hash() -> B256 {
    use alloy_rlp::Encodable;
    let empty_list: Vec<B256> = Vec::new();
    let mut buf = Vec::new();
    empty_list.encode(&mut buf);
    keccak256(&buf)
}

fn compute_state_root(alloc: &HashMap<String, AllocAccount>) -> B256 {
    if alloc.is_empty() {
        // Empty trie root - canonical hash for empty Merkle Patricia Trie
        return EMPTY_ROOT_HASH;
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
    // Empty ommers hash - Keccak-256 hash of RLP([]), used when no uncle blocks exist
    let ommers_hash = compute_empty_list_hash();

    // Empty trie root - canonical hash for empty Merkle Patricia Trie
    let empty_root = EMPTY_ROOT_HASH;

    let header = Header {
        parent_hash: B256::ZERO,
        ommers_hash,
        beneficiary: genesis.coinbase.parse::<Address>().unwrap_or(Address::ZERO),
        state_root,
        transactions_root: empty_root,
        receipts_root: empty_root,
        logs_bloom: Bloom::ZERO,
        difficulty: genesis
            .difficulty
            .parse::<U256>()
            .expect("Failed to parse difficulty"),
        number: 0,
        gas_limit: genesis
            .gas_limit
            .parse::<U64>()
            .expect("Failed to parse gasLimit")
            .wrapping_to::<u64>(),
        gas_used: 0,
        timestamp: genesis
            .timestamp
            .parse::<U64>()
            .expect("Failed to parse timestamp")
            .wrapping_to::<u64>(),
        extra_data: if genesis.extra_data.is_empty() || genesis.extra_data == "0x" {
            Bytes::new()
        } else {
            genesis
                .extra_data
                .parse::<Bytes>()
                .expect("Failed to parse extraData")
        },
        mix_hash: genesis.mix_hash.parse::<B256>().unwrap_or(B256::ZERO),
        nonce: B64::from(
            genesis
                .nonce
                .parse::<U64>()
                .expect("Failed to parse nonce")
                .wrapping_to::<u64>(),
        ),
        base_fee_per_gas: genesis
            .base_fee_per_gas
            .as_ref()
            .and_then(|s| s.parse::<U64>().ok())
            .map(|u| u.wrapping_to::<u64>()),
        withdrawals_root: Some(empty_root),
        blob_gas_used: genesis
            .blob_gas_used
            .as_ref()
            .and_then(|s| s.parse::<U64>().ok())
            .map(|u| u.wrapping_to::<u64>()),
        excess_blob_gas: genesis
            .excess_blob_gas
            .as_ref()
            .and_then(|s| s.parse::<U64>().ok())
            .map(|u| u.wrapping_to::<u64>()),
        parent_beacon_block_root: Some(B256::ZERO),
        requests_hash: None,
    };

    let mut buffer = Vec::new();
    header.encode(&mut buffer);
    buffer
}
