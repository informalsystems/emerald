use alloy_network::EthereumWallet;
use alloy_primitives::U256;
use alloy_provider::ProviderBuilder;
use alloy_signer_local::{coins_bip39::English, MnemonicBuilder};
use malachitebft_eth_utils::validator_set::contract::ValidatorSet;
use rand::Rng;
use malachitebft_eth_utils::validator_set::contract::GENESIS_VALIDATOR_SET_ACCOUNT;

const MNEMONIC: &str = "test test test test test test test test test test test junk";

const RPC_ENDPOINT: &str = "http://localhost:8545";

#[tokio::main]
async fn main() {
    let provider = ProviderBuilder::new().on_http(RPC_ENDPOINT.parse().unwrap());

    let validator_set_contract = ValidatorSet::new(GENESIS_VALIDATOR_SET_ACCOUNT, provider);

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
            ValidatorSet::new(GENESIS_VALIDATOR_SET_ACCOUNT, validator_provider);

        let count = validator_set_contract
            .getValidatorCount()
            .call()
            .await
            .unwrap()
            ._0;

        println!("Signer {i}: {address}");

        if validator_set_contract
            .getValidator(address)
            .call()
            .await
            .is_ok()
        {
            // unregister

            if count == U256::from(1u64) {
                println!("Only one validator left, skipping unregister");
                std::thread::sleep(std::time::Duration::from_secs(5));
                continue;
            }

            println!("Unregistering {address:?}");

            let _ = validator_set_contract
                .unregister()
                .send()
                .await
                .unwrap()
                .watch()
                .await
                .unwrap();
        } else {
            // register

            println!("Registering {address:?}");

            let old_info = original_validators
                .iter()
                .find(|v| v.validator == address)
                .unwrap();

            let _ = validator_set_contract
                .register(old_info.ed25519Key, old_info.power)
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
