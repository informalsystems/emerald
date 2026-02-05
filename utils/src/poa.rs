use alloy_network::EthereumWallet;
use alloy_primitives::{Address, U256};
use alloy_provider::ProviderBuilder;
use alloy_signer::utils::raw_public_key_to_address;
use alloy_signer_local::PrivateKeySigner;
use color_eyre::eyre;
use color_eyre::eyre::{Context, Result};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::PublicKey;
use reqwest::Url;

// Define the Solidity contract ABI
alloy_sol_types::sol!(
    #[derive(Debug)]
    #[sol(rpc)]
    ValidatorManager,
    "../solidity/out/ValidatorManager.sol/ValidatorManager.json"
);

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

pub fn validator_pubkey_to_address(validator_pubkey: &str) -> Result<Address> {
    let pubkey_bytes = hex::decode(
        validator_pubkey
            .strip_prefix("0x")
            .unwrap_or(validator_pubkey),
    )
    .context("Failed to decode validator public key")?;

    // Add 0x04 prefix if needed (uncompressed format)
    let pubkey_bytes_full: Vec<u8> = if pubkey_bytes.len() == 64 {
        let mut prefixed = Vec::with_capacity(65);
        prefixed.push(0x04);
        prefixed.extend_from_slice(&pubkey_bytes);
        prefixed
    } else if pubkey_bytes.len() == 65 {
        pubkey_bytes
    } else {
        eyre::bail!(
            "Invalid public key length: expected 64 or 65 bytes, got {}",
            pubkey_bytes.len()
        );
    };

    let pubkey = PublicKey::from_sec1_bytes(&pubkey_bytes_full)
        .map_err(|e| color_eyre::eyre::eyre!("Invalid public key bytes: {}", e))?;
    let addr = raw_public_key_to_address(&pubkey.to_encoded_point(false).as_bytes()[1..]);
    Ok(Address::from_slice(addr.as_slice()))
}

/// Parse validator identifier (either a public key or an address) and return an Address
///
/// Accepts:
/// - Public key: 64 bytes (128 hex chars) or 65 bytes (130 hex chars with 04 prefix)
/// - Address: 20 bytes (40 hex chars)
///
/// Both with or without 0x prefix
pub fn parse_validator_identifier(identifier: &str) -> Result<Address> {
    let hex_str = identifier.strip_prefix("0x").unwrap_or(identifier);
    let hex_len = hex_str.len();

    // Check if it's an address (40 hex chars = 20 bytes)
    if hex_len == 40 {
        // Parse as address
        let addr = identifier
            .parse::<Address>()
            .context("Failed to parse validator address")?;
        Ok(addr)
    } else if hex_len == 128 || hex_len == 130 {
        // Parse as public key (64 or 65 bytes)
        validator_pubkey_to_address(identifier)
    } else {
        eyre::bail!(
            "Invalid validator identifier: expected address (40 hex chars) or public key (128-130 hex chars), got {} hex chars",
            hex_len
        )
    }
}

// list validators
pub async fn list_validators(rpc_url: &Url, contract_address: &Address) -> Result<()> {
    let provider = ProviderBuilder::new().connect_http(rpc_url.clone());

    let contract = ValidatorManager::new(*contract_address, &provider);

    let poa_owner_address = contract.owner().call().await?.0;
    println!("POA Owner Address: 0x{poa_owner_address:x}");
    println!();

    let validators = contract.getValidators().call().await?;

    println!("Total validators: {}", validators.len());
    println!();

    // sort validators by power descending
    let mut validators = validators;
    validators.sort_by_key(|b| core::cmp::Reverse(b.power));

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
    validator_identifier: &str,
    power: u64,
    signer_private_key: &str,
) -> Result<()> {
    // Parse the validator public key bytes
    let hex_str = validator_identifier
        .strip_prefix("0x")
        .unwrap_or(validator_identifier);
    let pubkey_bytes = hex::decode(hex_str).context("Failed to decode validator public key")?;

    // Ensure the public key is in the correct format for the contract
    // Contract accepts: 33 bytes (compressed) or 65 bytes (uncompressed with 0x04 prefix)
    let validator_public_key_bytes: Vec<u8> = if pubkey_bytes.len() == 64 {
        // If 64 bytes, add the 0x04 prefix for uncompressed format
        let mut prefixed = Vec::with_capacity(65);
        prefixed.push(0x04);
        prefixed.extend_from_slice(&pubkey_bytes);
        prefixed
    } else if pubkey_bytes.len() == 65 || pubkey_bytes.len() == 33 || pubkey_bytes.len() == 20 {
        // Already in correct format (65 bytes uncompressed, 33 bytes compressed, or 20 bytes address)
        pubkey_bytes
    } else {
        return Err(color_eyre::eyre::eyre!(
            "Invalid input length: expected 20 (address), 33 (compressed key), 64, or 65 bytes (uncompressed key), got {}",
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
        .connect_http(rpc_url.clone());

    // Create contract instance
    let contract = ValidatorManager::new(*contract_address, &provider);

    // Call the register function
    println!("Adding validator with pubkey: {validator_identifier}");
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
/// Accepts either a validator public key or address
pub async fn remove_validator(
    rpc_url: &Url,
    contract_address: &Address,
    validator_identifier: &str,
    signer_private_key: &str,
) -> Result<()> {
    // Set up the signer and provider
    let signer: PrivateKeySigner = signer_private_key
        .parse()
        .context("Failed to parse private key")?;
    let wallet = EthereumWallet::from(signer);

    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(rpc_url.clone());

    // Create contract instance
    let contract = ValidatorManager::new(*contract_address, &provider);

    // Parse the validator identifier (pubkey or address)
    let addr = parse_validator_identifier(validator_identifier)?;

    // Call the unregister function
    println!("Removing validator: {validator_identifier}");
    println!("  Validator address: {addr:?}");

    let tx = contract
        .unregister(addr)
        .send()
        .await
        .context("Failed to send unregister transaction")?;

    println!("Transaction sent: {:?}", tx.tx_hash());

    let receipt = tx
        .get_receipt()
        .await
        .context("Failed to get transaction receipt")?;

    println!(
        "Transaction confirmed in block: {:?}",
        receipt.block_number.unwrap()
    );

    Ok(())
}

/// Update validator vote power
/// Accepts either a validator public key or address
pub async fn update_validator_power(
    rpc_url: &Url,
    contract_address: &Address,
    validator_identifier: &str,
    new_power: u64,
    signer_private_key: &str,
) -> Result<()> {
    // Set up the signer and provider
    let signer: PrivateKeySigner = signer_private_key
        .parse()
        .context("Failed to parse private key")?;
    let wallet = EthereumWallet::from(signer);

    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(rpc_url.clone());

    // Create contract instance
    let contract = ValidatorManager::new(*contract_address, &provider);

    // Parse the validator identifier (pubkey or address)
    let validator_address = parse_validator_identifier(validator_identifier)?;

    // Call the updatePower function
    println!("Updating validator power: {validator_identifier}");
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
