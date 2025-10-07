use alloy_network::EthereumWallet;
use alloy_primitives::U256;
use alloy_provider::ProviderBuilder;
use alloy_signer_local::{coins_bip39::English, MnemonicBuilder};
use malachitebft_eth_utils::validator_manager::contract::ValidatorManager;
use malachitebft_eth_utils::validator_manager::contract::GENESIS_VALIDATOR_MANAGER_ACCOUNT;
use rand::Rng;

const MNEMONIC: &str = "test test test test test test test test test test test junk";

const RPC_ENDPOINT: &str = "http://localhost:8545";

#[tokio::main]
async fn main() {
    let provider = ProviderBuilder::new().on_http(RPC_ENDPOINT.parse().unwrap());

    let validator_set_contract = ValidatorManager::new(GENESIS_VALIDATOR_MANAGER_ACCOUNT, provider);

    let original_validators = validator_set_contract
        .getValidators()
        .call()
        .await
        .unwrap()
        .validators;

    println!("Original validator set: {original_validators:?}");

    loop {
        let i = rand::thread_rng().gen_range(0..3);

        let signer = MnemonicBuilder::<English>::default()
            .phrase(MNEMONIC)
            .derivation_path(format!("m/44'/60'/0'/0/{i}"))
            .unwrap()
            .build()
            .unwrap();

        let address = signer.address();

        // Get the validator set from the alidatorSet contract at genesis block
        let validator_provider = ProviderBuilder::new()
            .wallet(EthereumWallet::from(signer.clone()))
            .on_http(RPC_ENDPOINT.parse().unwrap());

        let validator_set_contract =
            ValidatorManager::new(GENESIS_VALIDATOR_MANAGER_ACCOUNT, validator_provider);

        let count = validator_set_contract
            .getValidatorCount()
            .call()
            .await
            .unwrap()
            ._0;

        println!("Signer {i}: {address}");

        let validator_info = &original_validators[i];
        let validator_key = validator_info.validatorKey;

        let is_validator = validator_set_contract
            .isValidator(validator_key)
            .call()
            .await
            .unwrap()
            ._0;

        if is_validator {
            // unregister

            if count == U256::from(1u64) {
                println!("Only one validator left, skipping unregister");
                std::thread::sleep(std::time::Duration::from_secs(5));
                continue;
            }

            println!("Unregistering {address:?}");

            let _ = validator_set_contract
                .unregister(validator_key)
                .send()
                .await
                .unwrap()
                .watch()
                .await
                .unwrap();
        } else {
            // register

            println!("Registering {address:?}");

            let _ = validator_set_contract
                .register(validator_key, validator_info.power)
                .send()
                .await
                .unwrap()
                .watch()
                .await
                .unwrap();
        }

        std::thread::sleep(std::time::Duration::from_secs(
            rand::thread_rng().gen_range(5..10),
        ));
    }
}
