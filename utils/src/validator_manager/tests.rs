use std::io::ErrorKind;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::str::FromStr;

use crate::validator_manager::contract::ValidatorManager;

use super::{generate_storage_data, Validator};
use alloy_network::EthereumWallet;
use alloy_primitives::{address, keccak256, Address, B256, U256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_signer_local::PrivateKeySigner;
use ethers_signers::coins_bip39::English;
use ethers_signers::{LocalWallet, MnemonicBuilder};
use reqwest::Url;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};

/// Generate validators from "test test ... junk" mnemonic using sequential derivation paths.
///
/// Each validator is derived from path `m/44'/60'/0'/0/{index}` and includes both
/// the validator metadata and the associated ECDSA signing key required to submit
/// transactions on behalf of that validator.
const TEST_OWNER_ADDRESS: Address = address!("0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65");
const TEST_OWNER_PRIVATE_KEY: &str =
    "0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a";

fn generate_validators_from_mnemonic(count: usize) -> anyhow::Result<Vec<Validator>> {
    let mnemonic = "test test test test test test test test test test test junk";
    let mut derived = Vec::with_capacity(count);

    for i in 0..count {
        let derivation_path = format!("m/44'/60'/0'/0/{}", i);
        let wallet: LocalWallet = MnemonicBuilder::<English>::default()
            .phrase(mnemonic)
            .derivation_path(&derivation_path)?
            .build()?;

        let signing_key_hex = format!("0x{}", hex::encode(wallet.signer().to_bytes().as_slice()));
        let signer = PrivateKeySigner::from_str(&signing_key_hex)?;
        let address = signer.address();

        // Derive deterministic Ed25519 key by hashing the address and index together.
        let ed25519_key = {
            let mut entropy = Vec::with_capacity(Address::len_bytes() + 8);
            entropy.extend_from_slice(address.as_slice());
            entropy.extend_from_slice(&(i as u64).to_be_bytes());
            keccak256(entropy)
        };

        let power = U256::from(1000 * (i + 1));

        derived.push(Validator::from_public_key(B256::from(ed25519_key), power));
    }

    Ok(derived)
}

/// Deploy ValidatorManager contract on Anvil and compare storage values
///
/// This test attempts to deploy a ValidatorManager contract on a local Anvil node
/// and compare the generated storage values with the actual contract storage.
/// Currently disabled due to alloy v0.6 API changes requiring proper transaction
/// construction for contract deployment.
///
/// To manually test:
/// 1. Start Anvil: `anvil --host 0.0.0.0 --port 8545`
/// 2. Run: `cargo test anvil_integration_tests::test_anvil_storage_comparison -- --ignored`
#[tokio::test]
async fn test_anvil_storage_comparison() -> anyhow::Result<()> {
    let anvil = spawn_anvil().await?;

    println!("🚀 Starting Anvil storage comparison test");

    // Generate validators from mnemonic with sequential derivation paths
    let validators = generate_validators_from_mnemonic(5)?;
    println!("✅ Generated {} validators from mnemonic", validators.len());
    for (i, validator) in validators.iter().enumerate() {
        println!("   Validator {} key: {}", i, validator.validatorKey);
    }

    let expected_storage = generate_storage_data(validators.clone(), TEST_OWNER_ADDRESS)?;
    println!(
        "✅ Generated {} expected storage slots",
        expected_storage.len()
    );

    // Deploy contract and register validators on Anvil
    let rpc_url = anvil.rpc_url().clone();
    let contract_address =
        deploy_and_register_validators(&validators, TEST_OWNER_ADDRESS, &rpc_url).await?;
    println!(
        "✅ Contract deployed and validators registered at: {:#x}",
        contract_address
    );

    let provider = ProviderBuilder::new().on_http(rpc_url.clone());

    // Basic storage check - just verify non-empty storage exists
    let zero_slot = provider
        .get_storage_at(contract_address, U256::ZERO)
        .await?;
    println!("✅ Storage at slot 0: {}", zero_slot);

    for (slot, expected_value) in expected_storage.iter() {
        let actual_value = provider
            .get_storage_at(contract_address, (*slot).into())
            .await?;
        assert_eq!(
            actual_value,
            (*expected_value).into(),
            "Storage mismatch at slot {}",
            slot
        );
    }

    println!("🎉 Anvil integration test completed successfully!");
    println!("   Contract deployed and all storage slots match expected values.");
    println!(
        "   Expected {} storage slots verified.",
        expected_storage.len()
    );
    Ok(())
}

async fn deploy_and_register_validators(
    validators: &[Validator],
    owner: Address,
    rpc_endpoint: &Url,
) -> anyhow::Result<Address> {
    let deployer_key = PrivateKeySigner::from_str(TEST_OWNER_PRIVATE_KEY)?;
    debug_assert_eq!(deployer_key.address(), owner);
    let deployer_wallet = EthereumWallet::from(deployer_key);

    let deployer_provider = ProviderBuilder::new()
        .wallet(deployer_wallet)
        .on_http(rpc_endpoint.clone());

    // Deploy the contract using the generated bindings
    let deployed_contract = ValidatorManager::deploy(deployer_provider.clone()).await?;
    let contract_address = *deployed_contract.address();

    println!(
        "✅ Deployed ValidatorManager contract at: {:#x}",
        contract_address
    );

    // check bytecode exists at address
    let provider = ProviderBuilder::new().on_http(rpc_endpoint.clone());
    let code = provider.get_code_at(contract_address).await?;

    // assert bytecode matches
    assert_eq!(code, ValidatorManager::DEPLOYED_BYTECODE);

    println!("✅ Contract verified to have bytecode");

    let owner_contract = ValidatorManager::new(contract_address, deployer_provider.clone());
    for (i, validator) in validators.iter().enumerate() {
        let pending_tx = owner_contract
            .register(validator.validatorKey, validator.power)
            .send()
            .await?;

        let receipt = pending_tx.get_receipt().await?;
        if !receipt.status() {
            return Err(anyhow::anyhow!(
                "Failed to register validator {}: {:#x}",
                i,
                validator.validatorKey
            ));
        }
    }

    Ok(contract_address)
}

struct AnvilInstance {
    process: Child,
    rpc_url: Url,
    _temp_dir: TempDir,
}

impl AnvilInstance {
    fn rpc_url(&self) -> &Url {
        &self.rpc_url
    }
}

impl Drop for AnvilInstance {
    fn drop(&mut self) {
        if let Err(err) = self.process.kill() {
            if err.kind() != ErrorKind::InvalidInput {
                eprintln!("warning: failed to kill anvil process: {err}");
            }
        }
        let _ = self.process.wait();
    }
}

async fn spawn_anvil() -> anyhow::Result<AnvilInstance> {
    let temp_dir = TempDir::new()?;
    let port = reserve_port()?;

    let mut command = Command::new("anvil");
    command
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--accounts")
        .arg("200")
        .arg("--quiet")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.current_dir(temp_dir.path());

    let mut process = command.spawn()?;

    let rpc_url = Url::parse(&format!("http://127.0.0.1:{port}"))?;
    let provider = ProviderBuilder::new().on_http(rpc_url.clone());

    // Wait for Anvil to accept connections
    const MAX_ATTEMPTS: usize = 50;
    for attempt in 0..MAX_ATTEMPTS {
        match provider.get_block_number().await {
            Ok(_) => {
                return Ok(AnvilInstance {
                    process,
                    rpc_url,
                    _temp_dir: temp_dir,
                })
            }
            Err(err) if attempt + 1 == MAX_ATTEMPTS => {
                let _ = process.kill();
                let _ = process.wait();
                return Err(err.into());
            }
            Err(_) => sleep(Duration::from_millis(100)).await,
        }
    }

    unreachable!("wait loop should return or error out")
}

fn reserve_port() -> anyhow::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}
