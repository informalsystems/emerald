use alloy_genesis::{ChainConfig, Genesis, GenesisAccount};
use alloy_primitives::{address, keccak256, Address, Bytes, B256, U256};
use alloy_signer_local::{coins_bip39::English, LocalSigner, MnemonicBuilder};
use chrono::NaiveDate;
use color_eyre::eyre::Result;
use k256::ecdsa::SigningKey;
use std::{collections::BTreeMap, str::FromStr};

/// Test mnemonics for wallet generation
const TEST_MNEMONICS: [&str; 3] = [
    "test test test test test test test test test test test junk",
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    "zero zero zero zero zero zero zero zero zero zero zero zoo",
];

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

fn genesis_validator_set() -> Result<(Address, GenesisAccount)> {
    let contract_name = "ValidatorSet";
    let contract_address = address!("0000000000000000000000000000000000002000");

    struct InitialValidator {
        eth_address: Address,
        ed25519_public_key: U256,
        voting_power: u64,
    }

    let mut initial_validators = vec![
        InitialValidator {
            eth_address: address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
            // BUG FIX: Use from_str_radix for hex parsing, removing the "0x" prefix.
            ed25519_public_key: U256::from_str_radix(
                "1000000000000000000000000000000000000000000000000000000000000000",
                16,
            )?,
            voting_power: 110,
        },
        InitialValidator {
            eth_address: address!("70997970C51812dc3A010C7d01b50e0d17dc79C8"),
            ed25519_public_key: U256::from_str_radix(
                "2000000000000000000000000000000000000000000000000000000000000000",
                16,
            )?,
            voting_power: 150,
        },
        InitialValidator {
            eth_address: address!("3C44CdDdB6a900fa2b585dd299e03d12FA4293BC"),
            ed25519_public_key: U256::from_str_radix(
                "3000000000000000000000000000000000000000000000000000000000000000",
                16,
            )?,
            voting_power: 120,
        },
        InitialValidator {
            eth_address: address!("90F79bf6EB2c4f870365E785982E1f101E93b906"),
            ed25519_public_key: U256::from_str_radix(
                "4000000000000000000000000000000000000000000000000000000000000000",
                16,
            )?,
            voting_power: 130,
        },
    ];

    // BUG FIX: Validators MUST be sorted by address for the indexes to be correct.
    initial_validators.sort_by_key(|v| v.eth_address);

    let contract_json = format!("../solidity/out/{0}.sol/{0}.json", contract_name);
    let contract_artifact =
        serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(contract_json)?)?;
    let bytecode_hex = contract_artifact
        .pointer("/bytecode/object")
        .and_then(|v| v.as_str())
        .ok_or_else(|| color_eyre::eyre::eyre!("Bytecode not found in contract JSON"))?;

    let code = Bytes::from_str(bytecode_hex.trim())?;

    let mut storage = BTreeMap::new();

    let validators_map_slot = U256::from(0);
    let validator_addresses_array_slot = U256::from(1);
    let validator_index_map_slot = U256::from(2);

    // This part is correct and remains the same.
    storage.insert(
        B256::from(validator_addresses_array_slot),
        B256::from(U256::from(initial_validators.len())),
    );
    let array_data_start_slot = U256::from_be_bytes(
        keccak256(B256::from(validator_addresses_array_slot).as_slice()).into(),
    );
    for (i, validator) in initial_validators.iter().enumerate() {
        let array_element_slot = array_data_start_slot + U256::from(i);
        storage.insert(
            B256::from(array_element_slot),
            B256::from(U256::from_be_bytes(validator.eth_address.into_array())),
        );
    }

    // <<<< LOGIC UPDATED FOR NEW CONTRACT >>>>
    for (i, validator) in initial_validators.iter().enumerate() {
        // -- `validators` mapping (slot 0) --
        let validator_struct_base_slot_hash = keccak256(
            [
                validator.eth_address.as_slice(),
                &B256::from(validators_map_slot).0,
            ]
            .concat(),
        );
        let validator_struct_base_slot =
            U256::from_be_bytes(validator_struct_base_slot_hash.into());

        // According to the new struct, ed25519PublicKey is at the base slot.
        storage.insert(
            B256::from(validator_struct_base_slot),
            B256::from(validator.ed25519_public_key),
        );

        // votingPower is in the next slot.
        let voting_power_slot = validator_struct_base_slot + U256::from(1);
        storage.insert(
            B256::from(voting_power_slot),
            B256::from(U256::from(validator.voting_power)),
        );

        // -- `validatorAddressIndex` mapping (slot 2) -- This logic is still correct.
        let index_map_slot_hash = keccak256(
            [
                validator.eth_address.as_slice(),
                &B256::from(validator_index_map_slot).0,
            ]
            .concat(),
        );
        storage.insert(B256::from(index_map_slot_hash), B256::from(U256::from(i)));
    }

    let contract_account = GenesisAccount {
        balance: U256::ZERO,
        code: Some(code),
        storage: Some(storage),
        nonce: Some(0),
        ..Default::default()
    };

    Ok((contract_address, contract_account))
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

    let (address, account) = genesis_validator_set()?;

    alloc.insert(address, account);

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
