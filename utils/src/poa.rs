use alloy::{
    network::EthereumWallet,
    primitives::{Address, U256},
    providers::ProviderBuilder,
    signers::local::PrivateKeySigner,
    sol,
    transports::http::reqwest::Url,
};
use color_eyre::eyre::{Context, Result};

// Define the Solidity contract ABI
sol! {
    #[sol(rpc)]
    contract ValidatorManager {
        struct Secp256k1Key {
            uint256 x;
            uint256 y;
        }

        struct ValidatorInfo {
            Secp256k1Key validatorKey;
            uint64 power;
        }

        function register(Secp256k1Key memory validatorKey, uint64 power) external;
        function unregister(Secp256k1Key memory validatorKey) external;
        function getValidators() external view returns (ValidatorInfo[] memory validators);
    }
}

pub fn pubkey_parser(validator_pubkey: &str) -> Result<(U256, U256)> {
    let pubkey_bytes = hex::decode(
        validator_pubkey
            .strip_prefix("0x")
            .unwrap_or(validator_pubkey),
    )
    .context("Failed to decode validator public key")?;

    let (x_bytes, y_bytes) = if pubkey_bytes.len() == 65 && pubkey_bytes[0] == 0x04 {
        (&pubkey_bytes[1..33], &pubkey_bytes[33..65])
    } else if pubkey_bytes.len() == 64 {
        (&pubkey_bytes[0..32], &pubkey_bytes[32..64])
    } else {
        return Err(color_eyre::eyre::eyre!(
            "Invalid public key length: expected 64 or 65 bytes, got {}",
            pubkey_bytes.len()
        ));
    };

    let x = U256::from_be_slice(x_bytes);
    let y = U256::from_be_slice(y_bytes);

    Ok((x, y))
}

// list validators
pub async fn list_validators(rpc_url: Url, contract_address: Address) -> Result<()> {
    let provider = ProviderBuilder::new().on_http(rpc_url);

    let contract = ValidatorManager::new(contract_address, &provider);

    let validators = contract.getValidators().call().await?.validators;

    println!("Total validators: {}", validators.len());
    println!();

    for (i, validator) in validators.iter().enumerate() {
        println!("Validator #{}:", i + 1);
        println!("  X: {}", validator.validatorKey.x);
        println!("  Y: {}", validator.validatorKey.y);
        println!("  Power: {}", validator.power);
        println!();
    }

    Ok(())
}

// check_validator_exists
pub async fn check_validator_exists(
    rpc_url: Url,
    contract_address: Address,
    validator_private_key: &str,
) -> Result<bool> {
    let (x, y) = pubkey_parser(validator_private_key)?;
    let validator_key = ValidatorManager::Secp256k1Key { x, y };

    let provider = ProviderBuilder::new().on_http(rpc_url);

    let contract = ValidatorManager::new(contract_address, &provider);

    let is_validator = contract
        .getValidators()
        .call()
        .await?
        .validators
        .iter()
        .any(|v| v.validatorKey.x == validator_key.x && v.validatorKey.y == validator_key.y);

    Ok(is_validator)
}

/// Add a validator to the PoA validator set
pub async fn add_validator(
    rpc_url: Url,
    contract_address: Address,
    validator_pubkey: &str,
    power: u64,
    signer_private_key: &str,
) -> Result<()> {
    // Parse the validator public key (assuming it's a hex string of uncompressed secp256k1 key)
    let (x, y) = pubkey_parser(validator_pubkey)?;
    // Create the validator key struct
    let validator_key = ValidatorManager::Secp256k1Key { x, y };

    // Set up the signer and provider
    let signer: PrivateKeySigner = signer_private_key
        .parse()
        .context("Failed to parse private key")?;
    let wallet = EthereumWallet::from(signer);

    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .wallet(wallet)
        .on_http(rpc_url);

    // Create contract instance
    let contract = ValidatorManager::new(contract_address, &provider);

    // Call the register function
    println!("Adding validator with pubkey: {}", validator_pubkey);
    println!("  X: {}", x);
    println!("  Y: {}", y);
    println!("  Power: {}", power);

    let tx = contract
        .register(validator_key, power)
        .send()
        .await
        .context("Failed to send register transaction")?;

    println!("Transaction sent: {:?}", tx.tx_hash());

    let receipt = tx
        .get_receipt()
        .await
        .context("Failed to get transaction receipt")?;

    println!("Transaction confirmed in block: {:?}", receipt.block_number);
    println!("Gas used: {}", receipt.gas_used);

    Ok(())
}

/// Remove a validator from the PoA validator set
pub async fn remove_validator(
    rpc_url: Url,
    contract_address: Address,
    validator_pubkey: &str,
    signer_private_key: &str,
) -> Result<()> {
    // Parse the validator public key
    let pubkey_bytes = hex::decode(
        validator_pubkey
            .strip_prefix("0x")
            .unwrap_or(validator_pubkey),
    )
    .context("Failed to decode validator public key")?;

    let (x_bytes, y_bytes) = if pubkey_bytes.len() == 65 && pubkey_bytes[0] == 0x04 {
        (&pubkey_bytes[1..33], &pubkey_bytes[33..65])
    } else if pubkey_bytes.len() == 64 {
        (&pubkey_bytes[0..32], &pubkey_bytes[32..64])
    } else {
        return Err(color_eyre::eyre::eyre!(
            "Invalid public key length: expected 64 or 65 bytes, got {}",
            pubkey_bytes.len()
        ));
    };

    let x = U256::from_be_slice(x_bytes);
    let y = U256::from_be_slice(y_bytes);

    let validator_key = ValidatorManager::Secp256k1Key { x, y };

    // Set up the signer and provider
    let signer: PrivateKeySigner = signer_private_key
        .parse()
        .context("Failed to parse private key")?;
    let wallet = EthereumWallet::from(signer);

    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .wallet(wallet)
        .on_http(rpc_url);

    // Create contract instance
    let contract = ValidatorManager::new(contract_address, &provider);

    // Call the unregister function
    println!("Removing validator with pubkey: {}", validator_pubkey);

    let tx = contract
        .unregister(validator_key)
        .send()
        .await
        .context("Failed to send unregister transaction")?;

    println!("Transaction sent: {:?}", tx.tx_hash());

    let receipt = tx
        .get_receipt()
        .await
        .context("Failed to get transaction receipt")?;

    println!("Transaction confirmed in block: {:?}", receipt.block_number);

    Ok(())
}
