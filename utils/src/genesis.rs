use alloy_genesis::{ChainConfig, Genesis, GenesisAccount};
use alloy_primitives::{address, b256, Address, U256};
use alloy_signer_local::{coins_bip39::English, LocalSigner, MnemonicBuilder};
use chrono::NaiveDate;
use color_eyre::eyre::Result;
use k256::ecdsa::SigningKey;
use std::{collections::BTreeMap, str::FromStr};

use crate::validator_set::{self, Validator};

/// Test mnemonics for wallet generation
const TEST_MNEMONICS: [&str; 3] = [
    "test test test test test test test test test test test junk",
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    "zero zero zero zero zero zero zero zero zero zero zero zoo",
];

const GENESIS_VALIDATOR_SET_ACCOUNT: Address = address!("0000000000000000000000000000000000002000");

/// Create a signer from a mnemonic.
pub(crate) fn make_signer(mnemonic: &str) -> LocalSigner<SigningKey> {
    MnemonicBuilder::<English>::default()
        .phrase(mnemonic)
        .build()
        .expect("Failed to create wallet")
}

pub(crate) fn make_signers() -> Vec<LocalSigner<SigningKey>> {
    TEST_MNEMONICS
        .iter()
        .map(|&mnemonic| make_signer(mnemonic))
        .collect()
}

pub(crate) fn generate_genesis() -> Result<()> {
    let genesis_file = "./assets/genesis.json";

    // Create signers and get their addresses
    let signers = make_signers();
    let signer_addresses: Vec<Address> = signers.iter().map(|signer| signer.address()).collect();

    println!("Using signer addresses:");
    for (i, addr) in signer_addresses.iter().enumerate() {
        println!("Signer {i}: {addr}");
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

    let initial_validators = vec![
        Validator {
            address: address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
            ed25519_key: b256!("58501dc30e998b7874d03f5441c5e0952a8e9cfd896d5f68abc4648e4697c701"),
            power: U256::from(100),
        },
        Validator {
            address: address!("70997970C51812dc3A010C7d01b50e0d17dc79C8"),
            ed25519_key: b256!("75f904c0d021ec21f711e64add102b8a920b7dc0e6447c0998b181c7496d320f"),
            power: U256::from(120),
        },
        Validator {
            address: address!("3C44CdDdB6a900fa2b585dd299e03d12FA4293BC"),
            ed25519_key: b256!("6e864c490123a7b30ce8246ec893c326be160bd8c53e29e3614f35de565b3fec"),
            power: U256::from(110),
        },
    ];

    let storage = validator_set::generate_storage_data(initial_validators)?;

    alloy_sol_types::sol!(
        ValidatorSet,
        "../solidity/out/ValidatorSet.sol/ValidatorSet.json",
    );

    alloc.insert(
        GENESIS_VALIDATOR_SET_ACCOUNT,
        GenesisAccount {
            nonce: None,
            balance: U256::ZERO,
            code: Some(ValidatorSet::BYTECODE.clone()),
            storage: Some(storage),
            private_key: None,
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
