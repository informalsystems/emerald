use alloy_genesis::{ChainConfig, Genesis, GenesisAccount};
use alloy_primitives::{address, Address, B256, U256};
use alloy_signer_local::{coins_bip39::English, LocalSigner, MnemonicBuilder};
use chrono::NaiveDate;
use color_eyre::eyre::Result;
use k256::ecdsa::SigningKey;
use malachitebft_eth_types::PrivateKey;
use rand::{rngs::StdRng, SeedableRng};
use std::{collections::BTreeMap, str::FromStr};

use crate::validator_set::{contract::ValidatorSet, generate_storage_data, Validator};

/// Test mnemonic for wallet generation
const TEST_MNEMONIC: &str = "test test test test test test test test test test test junk";

const GENESIS_VALIDATOR_SET_ACCOUNT: Address = address!("0000000000000000000000000000000000002000");

/// Create a signer from a mnemonic.
pub(crate) fn make_signer(index: u64) -> LocalSigner<SigningKey> {
    MnemonicBuilder::<English>::default()
        .phrase(TEST_MNEMONIC)
        .derivation_path(format!("m/44'/60'/0'/0/{index}"))
        .expect("Failed to set derivation path")
        .build()
        .expect("Failed to create wallet")
}

pub(crate) fn make_signers() -> Vec<LocalSigner<SigningKey>> {
    (0..3).map(make_signer).collect()
}

pub(crate) fn generate_genesis() -> Result<()> {
    let genesis_file = "./assets/genesis.json";

    // Create signers and get their addresses
    let signers = make_signers();
    let signer_addresses: Vec<Address> = signers.iter().map(|signer| signer.address()).collect();

    println!("Using signer addresses:");
    for (i, (signer, addr)) in signers.iter().zip(signer_addresses.iter()).enumerate() {
        println!(
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

    let mut rng = StdRng::seed_from_u64(0x42);
    let private_keys: Vec<_> = (0..3).map(|_| PrivateKey::generate(&mut rng)).collect();

    let public_keys: Vec<_> = private_keys.iter().map(|pk| pk.public_key()).collect();

    let initial_validators = vec![
        Validator {
            address: address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
            ed25519_key: B256::from(public_keys[0].as_bytes()),
            power: U256::from(100),
        },
        Validator {
            address: address!("70997970C51812dc3A010C7d01b50e0d17dc79C8"),
            ed25519_key: B256::from(public_keys[1].as_bytes()),
            power: U256::from(120),
        },
        Validator {
            address: address!("3C44CdDdB6a900fa2b585dd299e03d12FA4293BC"),
            ed25519_key: B256::from(public_keys[2].as_bytes()),
            power: U256::from(110),
        },
    ];

    let storage = generate_storage_data(initial_validators)?;

    alloc.insert(
        GENESIS_VALIDATOR_SET_ACCOUNT,
        GenesisAccount {
            code: Some(ValidatorSet::DEPLOYED_BYTECODE.clone()),
            storage: Some(storage),
            ..Default::default()
        },
    );

    // The Ethereum Prague-Electra (Pectra) upgrade was activated on the mainnet
    // on May 7, 2025, at epoch 364,032.
    let date = NaiveDate::from_ymd_opt(2025, 5, 7).unwrap();
    let datetime = date.and_hms_opt(0, 0, 0).unwrap();
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
    std::fs::write(genesis_file, genesis_json)?;
    println!("Genesis configuration written to {genesis_file}");

    Ok(())
}
