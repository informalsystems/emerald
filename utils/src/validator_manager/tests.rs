use core::str::FromStr;
use std::{fs, path::Path};

use alloy_genesis::Genesis;
use alloy_network::EthereumWallet;
use alloy_node_bindings::anvil::Anvil;
use alloy_primitives::{address, Address, Bytes, U256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_signer_local::coins_bip39::English;
use alloy_signer_local::{MnemonicBuilder, PrivateKeySigner};
use alloy_sol_types::SolCall;
use color_eyre::eyre;
use reqwest::Url;
use tempfile::tempdir;
use tracing::debug;

use super::{
    generate_impl_storage, generate_storage_data, patched_impl_bytecode, Validator,
    ValidatorManager, ValidatorManagerProxy, GENESIS_VALIDATOR_MANAGER_ACCOUNT,
    GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT,
};
use crate::genesis::generate_evm_genesis;
use crate::validator_manager::storage::{
    erc7201_slot, EIP1967_IMPL_SLOT, INITIALIZABLE_NAMESPACE, INITIALIZABLE_SLOT,
    OWNABLE_NAMESPACE, OWNABLE_SLOT, REENTRANCY_GUARD_NAMESPACE, REENTRANCY_GUARD_SLOT,
};

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

// ---------------------------------------------------------------------------
// Golden tests: verify pinned ERC-7201 slot constants against the formula
// ---------------------------------------------------------------------------

#[test]
fn test_erc7201_ownable_slot() {
    assert_eq!(erc7201_slot(OWNABLE_NAMESPACE), OWNABLE_SLOT);
}

#[test]
fn test_erc7201_reentrancy_guard_slot() {
    assert_eq!(
        erc7201_slot(REENTRANCY_GUARD_NAMESPACE),
        REENTRANCY_GUARD_SLOT
    );
}

#[test]
fn test_erc7201_initializable_slot() {
    assert_eq!(erc7201_slot(INITIALIZABLE_NAMESPACE), INITIALIZABLE_SLOT);
}

#[test]
fn test_generate_evm_genesis_alloc_matches_expected_storage() -> eyre::Result<()> {
    let tmp = tempdir()?;
    let keys_path = tmp.path().join("validator_keys.txt");
    let genesis_path = tmp.path().join("genesis.json");

    let validators = generate_validators_from_mnemonic(5)?;
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

    let genesis: Genesis = serde_json::from_slice(&fs::read(&genesis_path)?)?;

    let proxy_account = genesis
        .alloc
        .get(&GENESIS_VALIDATOR_MANAGER_ACCOUNT)
        .ok_or_else(|| eyre::eyre!("missing proxy alloc entry at 0x2000"))?;
    let impl_account = genesis
        .alloc
        .get(&GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT)
        .ok_or_else(|| eyre::eyre!("missing implementation alloc entry at 0x2001"))?;

    assert_eq!(
        proxy_account.code.as_ref(),
        Some(&ValidatorManagerProxy::DEPLOYED_BYTECODE)
    );
    let expected_impl_bytecode = patched_impl_bytecode(GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT);
    assert_eq!(
        impl_account.code.as_ref(),
        Some(&expected_impl_bytecode)
    );

    let expected_proxy_storage = generate_storage_data(
        with_genesis_power(&validators, 100),
        TEST_OWNER_ADDRESS,
        GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT,
    )?;
    let expected_impl_storage = generate_impl_storage();

    assert_eq!(proxy_account.storage.as_ref(), Some(&expected_proxy_storage));
    assert_eq!(impl_account.storage.as_ref(), Some(&expected_impl_storage));

    assert_eq!(
        proxy_account
            .storage
            .as_ref()
            .and_then(|s| s.get(&EIP1967_IMPL_SLOT)),
        Some(&GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT.into_word())
    );
    assert_eq!(
        impl_account
            .storage
            .as_ref()
            .and_then(|s| s.get(&INITIALIZABLE_SLOT)),
        expected_impl_storage.get(&INITIALIZABLE_SLOT)
    );

    Ok(())
}

#[tokio::test]
#[test_log::test]
async fn test_anvil_boot_from_generated_genesis_proxy_and_impl_behavior() -> eyre::Result<()> {
    let tmp = tempdir()?;
    let keys_path = tmp.path().join("validator_keys.txt");
    let genesis_path = tmp.path().join("genesis.json");

    let validators = generate_validators_from_mnemonic(5)?;
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

    let impl_slot = provider
        .get_storage_at(GENESIS_VALIDATOR_MANAGER_ACCOUNT, EIP1967_IMPL_SLOT.into())
        .await?;
    assert_eq!(
        impl_slot.to_be_bytes::<32>(),
        GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT.into_word()
    );

    let vm_impl = ValidatorManager::new(GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT, &provider);

    // proxiableUUID has a `notDelegated` guard so it must be called directly on the
    // implementation, not through the proxy. It verifies __self == address(this), which
    // requires the patched bytecode to have the correct implementation address.
    let uuid = vm_impl.proxiableUUID().call().await?;
    assert_eq!(
        uuid,
        EIP1967_IMPL_SLOT,
        "proxiableUUID on implementation should return the EIP-1967 implementation slot"
    );
    let init_result = vm_impl.initialize(TEST_OWNER_ADDRESS).call().await;
    assert!(
        init_result.is_err(),
        "implementation initialize should revert when initializers are disabled"
    );

    Ok(())
}

#[tokio::test]
#[test_log::test]
async fn test_anvil_boot_from_generated_genesis_upgrade_succeeds()
-> eyre::Result<()> {
    let tmp = tempdir()?;
    let keys_path = tmp.path().join("validator_keys.txt");
    let genesis_path = tmp.path().join("genesis.json");

    let validators = generate_validators_from_mnemonic(5)?;
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
    let vm_proxy = ValidatorManager::new(GENESIS_VALIDATOR_MANAGER_ACCOUNT, &owner_provider);
    assert_eq!(vm_proxy.owner().call().await?, TEST_OWNER_ADDRESS);

    // Deploy a new UUPS-compatible implementation.
    let new_impl = ValidatorManager::deploy(owner_provider.clone()).await?;
    let new_impl_address = *new_impl.address();

    // Capture implementation slot before attempting the upgrade.
    let impl_before = owner_provider
        .get_storage_at(GENESIS_VALIDATOR_MANAGER_ACCOUNT, EIP1967_IMPL_SLOT.into())
        .await?;
    assert_eq!(
        impl_before.to_be_bytes::<32>(),
        GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT.into_word()
    );

    // Expected behavior: upgrade should succeed when called by owner.
    let receipt = vm_proxy
        .upgradeToAndCall(new_impl_address, Bytes::new())
        .send()
        .await?
        .get_receipt()
        .await?;
    assert!(receipt.status(), "upgrade transaction should succeed");

    // Implementation pointer should be updated to the new implementation.
    let impl_after = owner_provider
        .get_storage_at(GENESIS_VALIDATOR_MANAGER_ACCOUNT, EIP1967_IMPL_SLOT.into())
        .await?;
    assert_ne!(impl_after, impl_before);
    assert_eq!(impl_after.to_be_bytes::<32>(), new_impl_address.into_word());

    // Proxy state should remain intact after upgrade.
    assert_eq!(vm_proxy.owner().call().await?, TEST_OWNER_ADDRESS);
    assert_eq!(vm_proxy.getValidatorCount().call().await?, U256::from(5));
    assert_eq!(vm_proxy.getTotalPower().call().await?, 500u64);

    Ok(())
}

// ---------------------------------------------------------------------------
// Anvil integration: deploy behind proxy, compare storage
// ---------------------------------------------------------------------------

/// Deploy ValidatorManager behind an ERC1967 proxy on Anvil and compare
/// proxy storage against `generate_storage_data`.
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

    // Generate expected storage (same function genesis uses)
    let expected_storage =
        generate_storage_data(validators.clone(), TEST_OWNER_ADDRESS, impl_address)?;
    debug!("Generated {} expected storage slots", expected_storage.len());

    let provider = ProviderBuilder::new().connect_http(rpc_url.clone());

    for (slot, expected_value) in expected_storage.iter() {
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
