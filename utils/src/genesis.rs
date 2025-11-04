use std::collections::BTreeMap;
use std::str::FromStr;

use alloy_genesis::{ChainConfig, Genesis, GenesisAccount};
use alloy_primitives::{address, Address, B256, U256};
use alloy_signer_local::coins_bip39::English;
use alloy_signer_local::{MnemonicBuilder, PrivateKeySigner};
use chrono::NaiveDate;
use color_eyre::eyre::{eyre, Result};
use hex::decode;
use k256::ecdsa::VerifyingKey;
// Malachite types for Emerald genesis
use malachitebft_eth_types::secp256k1::PublicKey as EmeraldPublicKey;
use malachitebft_eth_types::{
    Genesis as EmeraldGenesis, Validator as EmeraldValidator, ValidatorSet as EmeraldValidatorSet,
};
use tracing::debug;

use crate::validator_manager::contract::{ValidatorManager, GENESIS_VALIDATOR_MANAGER_ACCOUNT};
use crate::validator_manager::{generate_storage_data, Validator};

/// EIP-4788 Beacon Roots Contract address
const BEACON_ROOTS_ADDRESS: Address = address!("0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02");

/// EIP-4788 Beacon Roots Contract bytecode
/// See: https://eips.ethereum.org/EIPS/eip-4788
const BEACON_ROOTS_CODE: &str = "0x3373fffffffffffffffffffffffffffffffffffffffe14604d57602036146024575f5ffd5b5f35801560495762001fff810690815414603c575f5ffd5b62001fff01545f5260205ff35b5f5ffd5b62001fff42064281555f359062001fff015500";

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

pub(crate) fn generate_genesis(
    public_keys_file: &str,
    poa_address_owner: &Option<String>,
    testnet: &bool,
    testnet_balance: &u64,
    chain_id: &u64,
    evm_genesis_output_file: &str,
    emerald_genesis_output_file: &str,
) -> Result<()> {
    generate_evm_genesis(
        public_keys_file,
        poa_address_owner,
        testnet,
        testnet_balance,
        chain_id,
        evm_genesis_output_file,
    )?;

    generate_emerald_genesis(public_keys_file, emerald_genesis_output_file)?;

    Ok(())
}

pub(crate) fn generate_evm_genesis(
    public_keys_file: &str,
    poa_address_owner: &Option<String>,
    testnet: &bool,
    testnet_balance: &u64,
    chain_id: &u64,
    genesis_output_file: &str,
) -> Result<()> {
    let mut alloc = BTreeMap::new();
    let signers = make_signers();
    // If test addresses are requested, create them and pre-fund them
    if *testnet {
        // Create signers and get their addresses
        let signer_addresses: Vec<Address> =
            signers.iter().map(|signer| signer.address()).collect();

        debug!("Using signer addresses:");
        for (i, (signer, addr)) in signers.iter().zip(signer_addresses.iter()).enumerate() {
            debug!(
                "Signer {i}: {addr} ({})",
                B256::from_slice(&signer.credential().to_bytes())
            );
        }

        let amount = U256::from(*testnet_balance) * U256::from(10).pow(U256::from(18));
        for addr in &signer_addresses {
            alloc.insert(
                *addr,
                GenesisAccount {
                    balance: amount,
                    ..Default::default()
                },
            );
        }
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

    // Parse PoA owner address or override with first test address
    let mut poa_address_owner = if let Some(addr_str) = poa_address_owner {
        Address::from_str(addr_str)
            .map_err(|e| eyre!("invalid PoA owner address '{}': {}", addr_str, e))?
    } else {
        Address::ZERO
    };

    if *testnet {
        poa_address_owner = signers[0].address();
    }

    let storage = generate_storage_data(initial_validators, poa_address_owner)?;
    alloc.insert(
        GENESIS_VALIDATOR_MANAGER_ACCOUNT,
        GenesisAccount {
            code: Some(ValidatorManager::DEPLOYED_BYTECODE.clone()),
            storage: Some(storage),
            ..Default::default()
        },
    );

    // Deploy EIP-4788 Beacon Roots Contract
    // Required for Engine API V3 compliance when parent_beacon_block_root is set
    // reth deploys this contract at genesis but only for chain-id 1 so we add it here manually in
    // order to support arbitrary chain-ids
    let beacon_roots_bytecode = hex::decode(BEACON_ROOTS_CODE.strip_prefix("0x").unwrap())?;
    alloc.insert(
        BEACON_ROOTS_ADDRESS,
        GenesisAccount {
            code: Some(beacon_roots_bytecode.into()),
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
            chain_id: *chain_id,
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

/// Generate Malachite/Emerald genesis file from validator public keys
pub(crate) fn generate_emerald_genesis(
    public_keys_file: &str,
    emerald_genesis_output_file: &str,
) -> Result<()> {
    debug!("Generating Emerald genesis file from {public_keys_file}");

    let mut validators = Vec::new();

    for (idx, raw_line) in std::fs::read_to_string(public_keys_file)?
        .lines()
        .enumerate()
    {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse hex-encoded public key (64 bytes without 0x04 prefix)
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

        // Convert to uncompressed SEC1 format (65 bytes with 0x04 prefix)
        let mut uncompressed = [0u8; 65];
        uncompressed[0] = 0x04;
        uncompressed[1..].copy_from_slice(&bytes);

        // Validate and create public key
        let pub_key = EmeraldPublicKey::from_sec1_bytes(&uncompressed).map_err(|_| {
            eyre!(
                "invalid secp256k1 public key material at line {} in {}",
                idx + 1,
                public_keys_file
            )
        })?;

        // Create validator with voting power of 1
        validators.push(EmeraldValidator::new(pub_key, 1));
    }

    if validators.is_empty() {
        return Err(eyre!("no valid validators found in {}", public_keys_file));
    }

    // Create validator set and genesis
    let validator_set = EmeraldValidatorSet::new(validators);
    let genesis = EmeraldGenesis { validator_set };

    // Write emerald genesis to file
    let genesis_json = serde_json::to_string_pretty(&genesis)?;
    std::fs::write(emerald_genesis_output_file, genesis_json)?;
    debug!("Emerald genesis configuration written to {emerald_genesis_output_file}");

    Ok(())
}
