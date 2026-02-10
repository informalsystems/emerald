use alloy_primitives::{address, Address};

/// Genesis validator manager account address
pub const GENESIS_ACCOUNT: Address = address!("0x0000000000000000000000000000000000002000");

alloy_sol_types::sol!(
    #[derive(Debug)]
    #[sol(rpc)]
    ValidatorManager,
    "../solidity/out/ValidatorManager.sol/ValidatorManager.json"
);

#[cfg(test)]
mod tests {
    use alloy_network::EthereumWallet;
    use alloy_node_bindings::Anvil;
    use alloy_primitives::{hex, Bytes, U256};
    use alloy_provider::ProviderBuilder;
    use alloy_signer_local::PrivateKeySigner;
    use color_eyre::Result;

    use super::ValidatorManager;

    const SECP256K1_G_UNCOMPRESSED: [u8; 65] = hex!(
        "04"
        "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
        "483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8"
    );

    const SECP256K1_G_X: [u8; 32] =
        hex!("79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798");

    const SECP256K1_G_Y: [u8; 32] =
        hex!("483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8");

    #[tokio::test]
    async fn deploy_register_unregister() -> Result<()> {
        let anvil = Anvil::new().spawn();
        let signer = PrivateKeySigner::from(&anvil.keys()[0]);
        let deployer = signer.address();
        let provider = ProviderBuilder::new()
            .wallet(EthereumWallet::from(signer))
            .connect_http(anvil.endpoint_url());

        let contract = ValidatorManager::deploy(provider).await?;

        assert_eq!(contract.owner().call().await?, deployer);
        assert_eq!(contract.getValidatorCount().call().await?, U256::ZERO);
        assert_eq!(contract.getTotalPower().call().await?, 0);

        let key = ValidatorManager::Secp256k1Key {
            x: U256::from_be_bytes(SECP256K1_G_X),
            y: U256::from_be_bytes(SECP256K1_G_Y),
        };
        let validator_addr = contract._validatorAddress(key).call().await?;

        let power = 42u64;
        let receipt = contract
            .register(Bytes::from(SECP256K1_G_UNCOMPRESSED.as_slice()), power)
            .send()
            .await?
            .get_receipt()
            .await?;
        assert!(receipt.status());

        assert_eq!(contract.getValidatorCount().call().await?, U256::from(1));
        assert_eq!(contract.getTotalPower().call().await?, power);
        assert!(contract.isValidator(validator_addr).call().await?);

        let info = contract.getValidator(validator_addr).call().await?;
        assert_eq!(info.power, power);
        assert_eq!(info.validatorKey.x, U256::from_be_bytes(SECP256K1_G_X));
        assert_eq!(info.validatorKey.y, U256::from_be_bytes(SECP256K1_G_Y));

        let receipt = contract
            .unregister(validator_addr)
            .send()
            .await?
            .get_receipt()
            .await?;
        assert!(receipt.status());

        assert_eq!(contract.getValidatorCount().call().await?, U256::ZERO);
        assert_eq!(contract.getTotalPower().call().await?, 0);
        assert!(!contract.isValidator(validator_addr).call().await?);

        Ok(())
    }
}
