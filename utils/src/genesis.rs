use core::str::FromStr;
use std::collections::BTreeMap;

use alloy_genesis::{ChainConfig, Genesis, GenesisAccount};
use alloy_primitives::{Address, B256, U256};
use alloy_signer_local::coins_bip39::English;
use alloy_signer_local::{MnemonicBuilder, PrivateKeySigner};
use chrono::NaiveDate;
use color_eyre::eyre::{eyre, Result};
use hex::decode;
use k256::ecdsa::VerifyingKey;
use tracing::debug;

use crate::validator_manager::contract::{ValidatorManager, GENESIS_VALIDATOR_MANAGER_ACCOUNT};
use crate::validator_manager::{generate_storage_data, Validator};

/// Test mnemonic for wallet generation
const TEST_MNEMONIC: &str = "test test test test test test test test test test test junk";

/// Create a signer from a mnemonic.
pub(crate) fn make_signer(index: u64) -> PrivateKeySigner {
    MnemonicBuilder::<English>::default()
        .phrase(TEST_MNEMONIC)
        .derivation_path(format!("m/44'/60'/0'/0/{index}"))
        .expect("Failed to set derivation path")
        .build()
        .expect("Failed to create wallet")
}

pub(crate) fn make_signers() -> Vec<PrivateKeySigner> {
    (0..10).map(make_signer).collect()
}

pub(crate) fn generate_genesis(public_keys_file: &str, genesis_output_file: &str) -> Result<()> {
    // Create signers and get their addresses
    let signers = make_signers();
    let signer_addresses: Vec<Address> = signers.iter().map(|signer| signer.address()).collect();

    debug!("Using signer addresses:");
    for (i, (signer, addr)) in signers.iter().zip(signer_addresses.iter()).enumerate() {
        debug!(
            "Signer {i}: {addr} ({})",
            B256::from_slice(&signer.credential().to_bytes())
        );
    }

    // Create genesis configuration with pre-funded accounts
    let mut alloc = BTreeMap::new();
    for addr in &signer_addresses {
        alloc.insert(
            *addr,
            GenesisAccount {
                balance: U256::from_str("15000000000000000000000").unwrap(), // 15000 ETH
                ..Default::default()
            },
        );
    }

    let mut initial_validators = Vec::new();
    for (idx, raw_line) in std::fs::read_to_string(public_keys_file)?
        .lines()
        .enumerate()
    {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let hex_str = line.strip_prefix("0x").unwrap_or(line);
        let bytes = decode(hex_str).map_err(|e| {
            eyre!(
                "invalid hex-encoded validator key at line {} in {}: {}",
                idx + 1,
                public_keys_file,
                e
            )
        })?;

        if bytes.len() != 64 {
            return Err(eyre!(
                "expected 64-byte uncompressed secp256k1 payload (sans 0x04 prefix) at line {} in {}, got {} bytes",
                idx + 1,
                public_keys_file,
                bytes.len()
            ));
        }

        let mut uncompressed = [0u8; 65];
        uncompressed[0] = 0x04;
        uncompressed[1..].copy_from_slice(&bytes);

        VerifyingKey::from_sec1_bytes(&uncompressed).map_err(|_| {
            eyre!(
                "invalid secp256k1 public key material at line {} in {}",
                idx + 1,
                public_keys_file
            )
        })?;

        let mut x_bytes = [0u8; 32];
        x_bytes.copy_from_slice(&bytes[..32]);
        let mut y_bytes = [0u8; 32];
        y_bytes.copy_from_slice(&bytes[32..]);
        let key = (U256::from_be_bytes(x_bytes), U256::from_be_bytes(y_bytes));
        initial_validators.push(Validator::from_public_key(key, 100));
    }

    let storage = generate_storage_data(initial_validators, signer_addresses[0])?;

    alloc.insert(
        GENESIS_VALIDATOR_MANAGER_ACCOUNT,
        GenesisAccount {
            code: Some(ValidatorManager::DEPLOYED_BYTECODE.clone()),
            storage: Some(storage),
            ..Default::default()
        },
    );

    // The Ethereum Prague-Electra (Pectra) upgrade was activated on the mainnet
    // on May 7, 2025, at epoch 364,032.
    let date = NaiveDate::from_ymd_opt(2025, 5, 7).expect("Failed to create date for May 7, 2025");
    let datetime = date
        .and_hms_opt(0, 0, 0)
        .expect("Failed to create datetime with 00:00:00");
    let valid_pectra_timestamp = datetime.and_utc().timestamp() as u64;

    // Create genesis configuration
    let genesis = Genesis {
        config: ChainConfig {
            chain_id: 1,
            homestead_block: Some(0),
            eip150_block: Some(0),
            eip155_block: Some(0),
            eip158_block: Some(0),
            byzantium_block: Some(0),
            constantinople_block: Some(0),
            petersburg_block: Some(0),
            istanbul_block: Some(0),
            berlin_block: Some(0),
            london_block: Some(0),
            shanghai_time: Some(0),
            cancun_time: Some(0),
            prague_time: Some(0),
            terminal_total_difficulty: Some(U256::ZERO),
            terminal_total_difficulty_passed: true,
            ..Default::default()
        },
        alloc,
        ..Default::default()
    }
    .with_gas_limit(30_000_000)
    .with_timestamp(valid_pectra_timestamp);

    // Create data directory if it doesn't exist
    std::fs::create_dir_all("./assets")?;

    // Write genesis to file
    let genesis_json = serde_json::to_string_pretty(&genesis)?;
    std::fs::write(genesis_output_file, genesis_json)?;
    debug!("Genesis configuration written to {genesis_output_file}");

    Ok(())
}
