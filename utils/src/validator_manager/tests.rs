use core::str::FromStr;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use alloy_genesis::Genesis;
use alloy_network::EthereumWallet;
use alloy_node_bindings::anvil::Anvil;
use alloy_primitives::{address, b256, Address, Bytes, U256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_signer_local::coins_bip39::English;
use alloy_signer_local::{MnemonicBuilder, PrivateKeySigner};
use alloy_sol_types::SolCall;
use color_eyre::eyre;
use reqwest::Url;
use tempfile::tempdir;
use tracing::debug;

use super::{
    Validator, ValidatorManager, ValidatorManagerProxy, GENESIS_VALIDATOR_MANAGER_ACCOUNT,
    GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT,
};
use crate::genesis::generate_evm_genesis;
use crate::revm_genesis::build_validator_manager_alloc_via_revm;

const TEST_OWNER_ADDRESS: Address = address!("0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65");
const TEST_OWNER_PRIVATE_KEY: &str =
    "0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a";
// ERC1967 implementation slot (keccak256("eip1967.proxy.implementation") - 1)
// Ref: https://eips.ethereum.org/EIPS/eip-1967
const EIP1967_IMPL_SLOT: alloy_primitives::B256 =
    b256!("360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc");

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

fn write_validator_keys_file(validators: &[Validator], path: &Path) -> eyre::Result<()> {
    let mut content = String::new();
    for validator in validators {
        let (x, y) = validator.validator_key;
        let mut raw = [0u8; 64];
        raw[..32].copy_from_slice(&x.to_be_bytes::<32>());
        raw[32..].copy_from_slice(&y.to_be_bytes::<32>());
        content.push_str(&hex::encode(raw));
        content.push('\n');
    }
    fs::write(path, content)?;
    Ok(())
}

fn with_genesis_power(validators: &[Validator], power: u64) -> Vec<Validator> {
    validators
        .iter()
        .map(|v| Validator::from_public_key(v.validator_key, power))
        .collect()
}

fn generate_test_genesis(validator_count: usize) -> eyre::Result<(tempfile::TempDir, Vec<Validator>, PathBuf)> {
    let tmp = tempdir()?;
    let keys_path = tmp.path().join("validator_keys.txt");
    let genesis_path = tmp.path().join("genesis.json");

    let validators = generate_validators_from_mnemonic(validator_count)?;
    write_validator_keys_file(&validators, &keys_path)?;

    let owner = Some(format!("{TEST_OWNER_ADDRESS:#x}"));
    let testnet = false;
    let testnet_balance = 0u64;
    let chain_id = 12345u64;
    generate_evm_genesis(
        keys_path
            .to_str()
            .ok_or_else(|| eyre::eyre!("validator keys path is not UTF-8"))?,
        &owner,
        &testnet,
        &testnet_balance,
        &chain_id,
        genesis_path
            .to_str()
            .ok_or_else(|| eyre::eyre!("genesis path is not UTF-8"))?,
    )?;

    Ok((tmp, validators, genesis_path))
}

#[test]
fn test_revm_alloc_rejects_empty_validators() {
    let err = build_validator_manager_alloc_via_revm(&[], TEST_OWNER_ADDRESS)
        .expect_err("empty validator list must fail");
    assert!(
        err.to_string().contains("validator list cannot be empty"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_revm_alloc_rejects_zero_power() -> eyre::Result<()> {
    let mut validators = generate_validators_from_mnemonic(2)?;
    validators[0].power = 0;

    let err = build_validator_manager_alloc_via_revm(&validators, TEST_OWNER_ADDRESS)
        .expect_err("zero power must fail");
    assert!(
        err.to_string().contains("has zero power"),
        "unexpected error: {err}"
    );
    Ok(())
}

#[test]
fn test_revm_alloc_rejects_duplicate_validator_keys() -> eyre::Result<()> {
    let mut validators = generate_validators_from_mnemonic(2)?;
    validators[1].validator_key = validators[0].validator_key;

    let err = build_validator_manager_alloc_via_revm(&validators, TEST_OWNER_ADDRESS)
        .expect_err("duplicate keys must fail");
    assert!(
        err.to_string().contains("duplicate validator key"),
        "unexpected error: {err}"
    );
    Ok(())
}

#[test]
fn test_revm_alloc_is_deterministic_for_same_inputs() -> eyre::Result<()> {
    let validators = generate_validators_from_mnemonic(5)?;

    let first = build_validator_manager_alloc_via_revm(&validators, TEST_OWNER_ADDRESS)?;
    let second = build_validator_manager_alloc_via_revm(&validators, TEST_OWNER_ADDRESS)?;

    assert_eq!(first.proxy_address, second.proxy_address);
    assert_eq!(first.implementation_address, second.implementation_address);
    assert_eq!(first.alloc, second.alloc);

    Ok(())
}

#[test]
fn test_revm_deployment_addresses_are_owner_independent() -> eyre::Result<()> {
    let validators = generate_validators_from_mnemonic(3)?;
    let owner_a = TEST_OWNER_ADDRESS;
    let owner_b = address!("0x70997970C51812dc3A010C7d01b50e0d17dc79C8");

    let alloc_a = build_validator_manager_alloc_via_revm(&validators, owner_a)?;
    let alloc_b = build_validator_manager_alloc_via_revm(&validators, owner_b)?;

    assert_eq!(alloc_a.proxy_address, alloc_b.proxy_address);
    assert_eq!(
        alloc_a.implementation_address,
        alloc_b.implementation_address
    );

    let genesis_deployer = address!("0x0000000000000000000000000000000000000001");
    assert!(!alloc_a.alloc.contains_key(&genesis_deployer));
    assert!(!alloc_b.alloc.contains_key(&genesis_deployer));
    assert!(!alloc_a.alloc.contains_key(&owner_a));
    assert!(!alloc_b.alloc.contains_key(&owner_b));

    Ok(())
}

#[test]
fn test_revm_deployment_addresses_match_published_constants() -> eyre::Result<()> {
    let validators = generate_validators_from_mnemonic(3)?;
    let alloc = build_validator_manager_alloc_via_revm(&validators, TEST_OWNER_ADDRESS)?;

    assert_eq!(alloc.proxy_address, GENESIS_VALIDATOR_MANAGER_ACCOUNT);
    assert_eq!(
        alloc.implementation_address,
        GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT
    );

    Ok(())
}

#[test]
fn test_generate_evm_genesis_alloc_matches_expected_storage() -> eyre::Result<()> {
    let (_tmp, validators, genesis_path) = generate_test_genesis(5)?;

    let genesis: Genesis = serde_json::from_slice(&fs::read(&genesis_path)?)?;

    let vm_genesis = build_validator_manager_alloc_via_revm(
        &with_genesis_power(&validators, 100),
        TEST_OWNER_ADDRESS,
    )?;

    assert_eq!(GENESIS_VALIDATOR_MANAGER_ACCOUNT, vm_genesis.proxy_address);
    assert_eq!(
        GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT,
        vm_genesis.implementation_address
    );

    let proxy_account = genesis
        .alloc
        .get(&GENESIS_VALIDATOR_MANAGER_ACCOUNT)
        .ok_or_else(|| eyre::eyre!("missing discovered proxy alloc entry"))?;
    let impl_account = genesis
        .alloc
        .get(&GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT)
        .ok_or_else(|| eyre::eyre!("missing discovered implementation alloc entry"))?;

    let expected_proxy_account = vm_genesis
        .alloc
        .get(&GENESIS_VALIDATOR_MANAGER_ACCOUNT)
        .ok_or_else(|| eyre::eyre!("missing expected proxy alloc entry"))?;
    let expected_impl_account = vm_genesis
        .alloc
        .get(&GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT)
        .ok_or_else(|| eyre::eyre!("missing expected implementation alloc entry"))?;

    assert_eq!(proxy_account, expected_proxy_account);
    assert_eq!(impl_account, expected_impl_account);

    Ok(())
}

#[tokio::test]
#[test_log::test]
async fn test_anvil_boot_from_generated_genesis_proxy_and_impl_behavior() -> eyre::Result<()> {
    let (_tmp, _validators, genesis_path) = generate_test_genesis(5)?;

    let genesis_path_str = genesis_path
        .to_str()
        .ok_or_else(|| eyre::eyre!("genesis path is not UTF-8"))?;
    let anvil = Anvil::new().args(["--init", genesis_path_str]).spawn();
    let rpc_url: Url = anvil.endpoint().parse()?;
    let provider = ProviderBuilder::new().connect_http(rpc_url);

    let vm_proxy = ValidatorManager::new(GENESIS_VALIDATOR_MANAGER_ACCOUNT, &provider);
    assert_eq!(vm_proxy.owner().call().await?, TEST_OWNER_ADDRESS);
    assert_eq!(vm_proxy.getValidatorCount().call().await?, U256::from(5));
    assert_eq!(vm_proxy.getTotalPower().call().await?, 500u64);
    assert_eq!(vm_proxy.getValidators().call().await?.len(), 5);

    let vm_impl = ValidatorManager::new(GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT, &provider);
    let init_result = vm_impl.initialize(TEST_OWNER_ADDRESS).call().await;
    assert!(
        init_result.is_err(),
        "implementation initialize should revert when initializers are disabled"
    );

    Ok(())
}

#[tokio::test]
#[test_log::test]
async fn test_anvil_boot_from_generated_genesis_upgrade_succeeds() -> eyre::Result<()> {
    let (_tmp, _validators, genesis_path) = generate_test_genesis(5)?;

    let genesis_path_str = genesis_path
        .to_str()
        .ok_or_else(|| eyre::eyre!("genesis path is not UTF-8"))?;
    let anvil = Anvil::new().args(["--init", genesis_path_str]).spawn();
    let rpc_url: Url = anvil.endpoint().parse()?;

    // Owner wallet provider (same key as genesis owner) so the upgrade path
    // reaches UUPS call-context checks instead of failing onlyOwner().
    let owner_key = PrivateKeySigner::from_str(TEST_OWNER_PRIVATE_KEY)?;
    debug_assert_eq!(owner_key.address(), TEST_OWNER_ADDRESS);
    let owner_provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(owner_key))
        .connect_http(rpc_url);

    // Sanity check: proxy is initialized and owned by the expected account.
    let proxy_address = GENESIS_VALIDATOR_MANAGER_ACCOUNT;

    let vm_proxy = ValidatorManager::new(proxy_address, &owner_provider);
    assert_eq!(vm_proxy.owner().call().await?, TEST_OWNER_ADDRESS);

    // Deploy a new UUPS-compatible implementation.
    let new_impl = ValidatorManager::deploy(owner_provider.clone()).await?;
    let new_impl_address = *new_impl.address();

    // Expected behavior: upgrade should succeed when called by owner.
    let receipt = vm_proxy
        .upgradeToAndCall(new_impl_address, Bytes::new())
        .send()
        .await?
        .get_receipt()
        .await?;
    assert!(receipt.status(), "upgrade transaction should succeed");

    // Proxy state should remain intact after upgrade.
    assert_eq!(vm_proxy.owner().call().await?, TEST_OWNER_ADDRESS);
    assert_eq!(vm_proxy.getValidatorCount().call().await?, U256::from(5));
    assert_eq!(vm_proxy.getTotalPower().call().await?, 500u64);

    Ok(())
}

// ---------------------------------------------------------------------------
// Anvil integration: deploy behind proxy, compare storage
// ---------------------------------------------------------------------------

#[tokio::test]
#[test_log::test]
async fn test_anvil_storage_comparison() -> eyre::Result<()> {
    let anvil = Anvil::new().spawn();
    let rpc_url: Url = anvil.endpoint().parse()?;

    debug!("Starting Anvil storage comparison test");

    let validators = generate_validators_from_mnemonic(5)?;
    debug!("Generated {} validators from mnemonic", validators.len());

    // Deploy implementation + proxy via transactions (normal deploy path)
    let (proxy_address, impl_address) =
        deploy_proxy_and_register_validators(&validators, TEST_OWNER_ADDRESS, &rpc_url).await?;
    debug!("Proxy at {proxy_address:#x}, impl at {impl_address:#x}");

    let vm_genesis = build_validator_manager_alloc_via_revm(&validators, TEST_OWNER_ADDRESS)?;
    let expected_storage = vm_genesis
        .alloc
        .get(&vm_genesis.proxy_address)
        .and_then(|account| account.storage.as_ref())
        .ok_or_else(|| eyre::eyre!("missing expected proxy storage in revm alloc"))?;
    debug!(
        "Generated {} expected storage slots",
        expected_storage.len()
    );

    let provider = ProviderBuilder::new().connect_http(rpc_url.clone());

    for (slot, expected_value) in expected_storage.iter() {
        // Skip ERC1967 impl pointer: fresh anvil deployment and genesis alloc can
        // point to different impl addresses while behavior is still valid.
        if *slot == EIP1967_IMPL_SLOT {
            continue;
        }
        let actual_value = provider
            .get_storage_at(proxy_address, (*slot).into())
            .await?;
        assert_eq!(
            actual_value.to_be_bytes::<32>(),
            (*expected_value),
            "Storage mismatch at slot {slot}",
        );
    }

    debug!(
        "Anvil storage comparison passed: {} slots verified.",
        expected_storage.len()
    );
    Ok(())
}

/// Deploy implementation, proxy(impl, initData), then register validators.
async fn deploy_proxy_and_register_validators(
    validators: &[Validator],
    owner: Address,
    rpc_endpoint: &Url,
) -> eyre::Result<(Address, Address)> {
    let deployer_key = PrivateKeySigner::from_str(TEST_OWNER_PRIVATE_KEY)?;
    debug_assert_eq!(deployer_key.address(), owner);
    let deployer_wallet = EthereumWallet::from(deployer_key);

    let deployer_provider = ProviderBuilder::new()
        .wallet(deployer_wallet)
        .connect_http(rpc_endpoint.clone());

    // 1. Deploy implementation
    let impl_contract = ValidatorManager::deploy(deployer_provider.clone()).await?;
    let impl_address = *impl_contract.address();
    debug!("Deployed implementation at {impl_address:#x}");

    // 2. Deploy proxy with initialize calldata
    let init_data = ValidatorManager::initializeCall {
        initialOwner: owner,
    }
    .abi_encode();
    let proxy_contract = ValidatorManagerProxy::deploy(
        deployer_provider.clone(),
        impl_address,
        Bytes::from(init_data),
    )
    .await?;
    let proxy_address = *proxy_contract.address();
    debug!("Deployed proxy at {proxy_address:#x}");

    // 3. Register validators through the proxy
    let vm = ValidatorManager::new(proxy_address, deployer_provider.clone());
    for (i, validator) in validators.iter().enumerate() {
        let info: ValidatorManager::ValidatorInfo = validator.clone().into();
        let mut pubkey_bytes = Vec::with_capacity(65);
        pubkey_bytes.push(0x04);
        pubkey_bytes.extend_from_slice(&info.validatorKey.x.to_be_bytes::<32>());
        pubkey_bytes.extend_from_slice(&info.validatorKey.y.to_be_bytes::<32>());

        let receipt = vm
            .register(pubkey_bytes.into(), info.power)
            .send()
            .await?
            .get_receipt()
            .await?;
        if !receipt.status() {
            return Err(eyre::anyhow!(
                "Failed to register validator {}: ({:#x}, {:#x})",
                i,
                validator.validator_key.0,
                validator.validator_key.1
            ));
        }
    }

    let total_power = vm.getTotalPower().call().await?;
    debug!("On-chain total power: {total_power:?}");

    Ok((proxy_address, impl_address))
}
