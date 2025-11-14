use core::str::FromStr;

use alloy_network::EthereumWallet;
use alloy_node_bindings::anvil::Anvil;
use alloy_primitives::{address, Address, U256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_signer_local::coins_bip39::English;
use alloy_signer_local::{MnemonicBuilder, PrivateKeySigner};
use color_eyre::eyre;
use reqwest::Url;
use tracing::debug;

use super::{generate_storage_data, Validator};
use crate::validator_manager::contract::ValidatorManager;

fn encode_uncompressed_key(key: ValidatorManager::Secp256k1Key) -> Vec<u8> {
    let mut out = Vec::with_capacity(65);
    out.push(0x04);
    out.extend_from_slice(&key.x.to_be_bytes::<32>());
    out.extend_from_slice(&key.y.to_be_bytes::<32>());
    out
}

/// Generate validators from "test test ... junk" mnemonic using sequential derivation paths.
///
/// Each validator is derived from path `m/44'/60'/0'/0/{index}` and includes both
/// the validator metadata and the associated ECDSA signing key required to submit
/// transactions on behalf of that validator.
const TEST_OWNER_ADDRESS: Address = address!("0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65");
const TEST_OWNER_PRIVATE_KEY: &str =
    "0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a";

fn generate_validators_from_mnemonic(count: usize) -> eyre::Result<Vec<Validator>> {
    let mnemonic = "test test test test test test test test test test test junk";
    let mut derived = Vec::with_capacity(count);

    for i in 0..count {
        let derivation_path = format!("m/44'/60'/0'/0/{i}");
        let wallet = MnemonicBuilder::<English>::default()
            .phrase(mnemonic)
            .derivation_path(&derivation_path)?
            .build()?;

        let verifying_key = wallet.credential().verifying_key();
        let encoded = verifying_key.to_encoded_point(false);
        let pubkey_bytes = encoded.as_bytes();
        debug_assert_eq!(
            pubkey_bytes.len(),
            65,
            "secp256k1 uncompressed key must be 65 bytes"
        );

        let mut x_bytes = [0u8; 32];
        x_bytes.copy_from_slice(&pubkey_bytes[1..33]);
        let mut y_bytes = [0u8; 32];
        y_bytes.copy_from_slice(&pubkey_bytes[33..]);
        let validator_key = (U256::from_be_bytes(x_bytes), U256::from_be_bytes(y_bytes));
        let power = (1000 * (i + 1)) as u64;

        derived.push(Validator::from_public_key(validator_key, power));
    }

    Ok(derived)
}

/// Deploy ValidatorManager contract on Anvil and compare storage values
///
/// This test attempts to deploy a ValidatorManager contract on a local Anvil node
/// and compare the generated storage values with the actual contract storage.
#[tokio::test]
#[test_log::test]
async fn test_anvil_storage_comparison() -> eyre::Result<()> {
    let anvil = Anvil::new().spawn();
    let rpc_url: Url = anvil.endpoint().parse()?;

    debug!("🚀 Starting Anvil storage comparison test");

    // Generate validators from mnemonic with sequential derivation paths
    let validators = generate_validators_from_mnemonic(5)?;
    debug!("✅ Generated {} validators from mnemonic", validators.len());
    for (i, validator) in validators.iter().enumerate() {
        debug!(
            "   Validator {} key: ({:#x}, {:#x})",
            i, validator.validator_key.0, validator.validator_key.1
        );
    }

    let expected_storage = generate_storage_data(validators.clone(), TEST_OWNER_ADDRESS)?;
    debug!(
        "✅ Generated {} expected storage slots",
        expected_storage.len()
    );

    // Deploy contract and register validators on Anvil
    let contract_address =
        deploy_and_register_validators(&validators, TEST_OWNER_ADDRESS, &rpc_url).await?;
    debug!(
        "✅ Contract deployed and validators registered at: {:#x}",
        contract_address
    );

    let provider = ProviderBuilder::new().on_http(rpc_url.clone());

    // Basic storage check - just verify non-empty storage exists
    let zero_slot = provider
        .get_storage_at(contract_address, U256::ZERO)
        .await?;
    debug!("✅ Storage at slot 0: {}", zero_slot);

    for (slot, expected_value) in expected_storage.iter() {
        let actual_value = provider
            .get_storage_at(contract_address, (*slot).into())
            .await?;
        assert_eq!(
            actual_value,
            (*expected_value).into(),
            "Storage mismatch at slot {slot}",
        );
    }

    debug!("🎉 Anvil integration test completed successfully!");
    debug!("   Contract deployed and all storage slots match expected values.");
    debug!(
        "   Expected {} storage slots verified.",
        expected_storage.len()
    );
    Ok(())
}

async fn deploy_and_register_validators(
    validators: &[Validator],
    owner: Address,
    rpc_endpoint: &Url,
) -> eyre::Result<Address> {
    let deployer_key = PrivateKeySigner::from_str(TEST_OWNER_PRIVATE_KEY)?;
    debug_assert_eq!(deployer_key.address(), owner);
    let deployer_wallet = EthereumWallet::from(deployer_key);

    let deployer_provider = ProviderBuilder::new()
        .wallet(deployer_wallet)
        .on_http(rpc_endpoint.clone());

    // Deploy the contract using the generated bindings
    let deployed_contract = ValidatorManager::deploy(deployer_provider.clone()).await?;
    let contract_address = *deployed_contract.address();

    debug!(
        "✅ Deployed ValidatorManager contract at: {:#x}",
        contract_address
    );

    // check bytecode exists at address
    let provider = ProviderBuilder::new().on_http(rpc_endpoint.clone());
    let code = provider.get_code_at(contract_address).await?;

    // assert bytecode matches
    assert_eq!(code, ValidatorManager::DEPLOYED_BYTECODE);

    debug!("✅ Contract verified to have bytecode");

    let owner_contract = ValidatorManager::new(contract_address, deployer_provider.clone());
    for (i, validator) in validators.iter().enumerate() {
        let info: ValidatorManager::ValidatorInfo = validator.clone().into();
        let pubkey_bytes = encode_uncompressed_key(info.validatorKey);
        let pending_tx = owner_contract
            .register(pubkey_bytes.into(), info.power)
            .send()
            .await?;

        let receipt = pending_tx.get_receipt().await?;
        if !receipt.status() {
            return Err(eyre::anyhow!(
                "Failed to register validator {}: ({:#x}, {:#x})",
                i,
                validator.validator_key.0,
                validator.validator_key.1
            ));
        }
    }

    let onchain_total_power = owner_contract.getTotalPower().call().await?;
    debug!("✅ On-chain total power after registration: {onchain_total_power:?}",);

    Ok(contract_address)
}
