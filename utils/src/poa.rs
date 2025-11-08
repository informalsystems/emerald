use alloy_network::EthereumWallet;
use alloy_primitives::{Address, U256};
use alloy_provider::ProviderBuilder;
use alloy_signer::utils::raw_public_key_to_address;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::sol;
use color_eyre::eyre;
use color_eyre::eyre::{Context, Result};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::PublicKey;
use reqwest::Url;

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

        function register(bytes calldata validatorPublicKey, uint64 power) external;
        function unregister(address validatorAddress) external;
        function updatePower(address validatorAddress, uint64 newPower) external;
        function getValidators() external view returns (ValidatorInfo[] memory validators);
        function _validatorAddress(Secp256k1Key memory validatorKey) external pure returns (address);
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
        eyre::bail!(
            "Invalid public key length, expected 64 or 65 bytes, got {}",
            pubkey_bytes.len()
        );
    };

    let x = U256::from_be_slice(x_bytes);
    let y = U256::from_be_slice(y_bytes);

    Ok((x, y))
}

// list validators
pub async fn list_validators(rpc_url: &Url, contract_address: &Address) -> Result<()> {
    let provider = ProviderBuilder::new().on_http(rpc_url.clone());

    let contract = ValidatorManager::new(*contract_address, &provider);

    let validators = contract.getValidators().call().await?.validators;

    println!("Total validators: {}", validators.len());
    println!();

    // sort validators by power descending
    let mut validators = validators;
    validators.sort_by(|a, b| b.power.cmp(&a.power));

    for (i, validator) in validators.iter().enumerate() {
        println!("Validator #{}:", i + 1);
        println!("  Power: {}", validator.power);
        // validator pubkey in hex
        let mut pubkey_bytes = Vec::with_capacity(65);
        pubkey_bytes.push(0x04); // uncompressed prefix
        pubkey_bytes.extend_from_slice(&validator.validatorKey.x.to_be_bytes::<32>());
        pubkey_bytes.extend_from_slice(&validator.validatorKey.y.to_be_bytes::<32>());
        println!("  Pubkey: {}", hex::encode(&pubkey_bytes));
        // print validator address 0x
        let pubkey = PublicKey::from_sec1_bytes(&pubkey_bytes)
            .map_err(|e| color_eyre::eyre::eyre!("Invalid public key bytes: {}", e))?;
        let address = raw_public_key_to_address(&pubkey.to_encoded_point(false).as_bytes()[1..]);
        println!("Validator address: 0x{address:x}");
        println!();
    }

    Ok(())
}

/// Add a validator to the PoA validator set
pub async fn add_validator(
    rpc_url: &Url,
    contract_address: &Address,
    validator_pubkey: &str,
    power: u64,
    signer_private_key: &str,
) -> Result<()> {
    // Parse the validator public key bytes
    let pubkey_bytes = hex::decode(
        validator_pubkey
            .strip_prefix("0x")
            .unwrap_or(validator_pubkey),
    )
    .context("Failed to decode validator public key")?;

    // Ensure the public key is in the correct format for the contract
    // Contract accepts: 33 bytes (compressed) or 65 bytes (uncompressed with 0x04 prefix)
    let validator_public_key_bytes: Vec<u8> = if pubkey_bytes.len() == 64 {
        // If 64 bytes, add the 0x04 prefix for uncompressed format
        let mut prefixed = Vec::with_capacity(65);
        prefixed.push(0x04);
        prefixed.extend_from_slice(&pubkey_bytes);
        prefixed
    } else if pubkey_bytes.len() == 65 || pubkey_bytes.len() == 33 {
        // Already in correct format (65 bytes uncompressed or 33 bytes compressed)
        pubkey_bytes
    } else {
        return Err(color_eyre::eyre::eyre!(
            "Invalid public key length: expected 33, 64, or 65 bytes, got {}",
            pubkey_bytes.len()
        ));
    };

    // Set up the signer and provider
    let signer: PrivateKeySigner = signer_private_key
        .parse()
        .context("Failed to parse private key")?;
    let wallet = EthereumWallet::from(signer);

    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .on_http(rpc_url.clone());

    // Create contract instance
    let contract = ValidatorManager::new(*contract_address, &provider);

    // Call the register function
    println!("Adding validator with pubkey: {validator_pubkey}");
    println!("  Power: {power}");

    let tx = contract
        .register(validator_public_key_bytes.into(), power)
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
    rpc_url: &Url,
    contract_address: &Address,
    validator_pubkey: &str,
    signer_private_key: &str,
) -> Result<()> {
    // Parse the validator public key to get (x, y)
    let (x, y) = pubkey_parser(validator_pubkey)?;
    let validator_key = ValidatorManager::Secp256k1Key { x, y };

    // Set up the signer and provider
    let signer: PrivateKeySigner = signer_private_key
        .parse()
        .context("Failed to parse private key")?;
    let wallet = EthereumWallet::from(signer);

    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .on_http(rpc_url.clone());

    // Create contract instance
    let contract = ValidatorManager::new(*contract_address, &provider);

    // Get the validator address from the contract
    let validator_address = contract
        ._validatorAddress(validator_key)
        .call()
        .await
        .context("Failed to get validator address")?
        ._0;

    // Call the unregister function
    println!("Removing validator with pubkey: {validator_pubkey}");
    println!("  Validator address: {validator_address:?}");

    let tx = contract
        .unregister(validator_address)
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

// update validator vote power
pub async fn update_validator_power(
    rpc_url: &Url,
    contract_address: &Address,
    validator_pubkey: &str,
    new_power: u64,
    signer_private_key: &str,
) -> Result<()> {
    // Parse the validator public key to get (x, y)
    let (x, y) = pubkey_parser(validator_pubkey)?;
    let validator_key = ValidatorManager::Secp256k1Key { x, y };

    // Set up the signer and provider
    let signer: PrivateKeySigner = signer_private_key
        .parse()
        .context("Failed to parse private key")?;
    let wallet = EthereumWallet::from(signer);

    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .on_http(rpc_url.clone());

    // Create contract instance
    let contract = ValidatorManager::new(*contract_address, &provider);

    // Get the validator address from the contract
    let validator_address = contract
        ._validatorAddress(validator_key)
        .call()
        .await
        .context("Failed to get validator address")?
        ._0;

    // Call the updatePower function
    println!("Updating validator power with pubkey: {validator_pubkey}");
    println!("  Validator address: {validator_address:?}");
    println!("  New power: {new_power}");

    let tx = contract
        .updatePower(validator_address, new_power)
        .send()
        .await
        .context("Failed to send updatePower transaction")?;

    println!("Transaction sent: {:?}", tx.tx_hash());

    let receipt = tx
        .get_receipt()
        .await
        .context("Failed to get transaction receipt")?;

    println!("Transaction confirmed in block: {receipt:?}");
    println!("Gas used: {}", receipt.gas_used);

    Ok(())
}
