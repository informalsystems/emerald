use alloy_primitives::{address, Address, U256};
use alloy_provider::ProviderBuilder;
use color_eyre::eyre;
use malachitebft_eth_types::secp256k1::PublicKey;
use malachitebft_eth_types::{BlockHash, Validator, ValidatorSet};

const GENESIS_VALIDATOR_MANAGER_ACCOUNT: Address =
    address!("0x0000000000000000000000000000000000002000");

alloy_sol_types::sol!(
    #[derive(Debug)]
    #[sol(rpc)]
    ValidatorManager,
    "../solidity/out/ValidatorManager.sol/ValidatorManager.json"
);

/// Parse a validator's uncompressed SEC1 public key from x and y coordinates.
fn parse_validator_public_key(x: &U256, y: &U256) -> eyre::Result<PublicKey> {
    let mut uncompressed = [0u8; 65];
    uncompressed[0] = 0x04;
    uncompressed[1..33].copy_from_slice(&x.to_be_bytes::<32>());
    uncompressed[33..].copy_from_slice(&y.to_be_bytes::<32>());

    Ok(PublicKey::from_sec1_bytes(&uncompressed)?)
}

/// Convert contract validator info into domain validators.
fn parse_validators(
    validator_infos: Vec<ValidatorManager::ValidatorInfo>,
) -> eyre::Result<Vec<Validator>> {
    validator_infos
        .into_iter()
        .map(|info| {
            let pub_key = parse_validator_public_key(&info.validatorKey.x, &info.validatorKey.y)?;
            Ok(Validator::new(pub_key, info.power))
        })
        .collect()
}

pub async fn read_validators_from_contract(
    eth_url: &str,
    block_hash: &BlockHash,
) -> eyre::Result<ValidatorSet> {
    let provider = ProviderBuilder::new().connect(eth_url).await?;

    let validator_manager_contract =
        ValidatorManager::new(GENESIS_VALIDATOR_MANAGER_ACCOUNT, provider);

    let genesis_validator_set_sol = validator_manager_contract
        .getValidators()
        .block((*block_hash).into())
        .call()
        .await?;

    let validators = parse_validators(genesis_validator_set_sol)?;

    Ok(ValidatorSet::new(validators))
}

#[cfg(test)]
mod tests {
    use alloy_primitives::U256;

    use super::*;

    /// Helper to create a ValidatorInfo from hex-encoded x, y coordinates and power.
    fn make_validator_info(
        x_hex: &str,
        y_hex: &str,
        power: u64,
    ) -> ValidatorManager::ValidatorInfo {
        ValidatorManager::ValidatorInfo {
            validatorKey: ValidatorManager::Secp256k1Key {
                x: U256::from_str_radix(x_hex, 16).unwrap(),
                y: U256::from_str_radix(y_hex, 16).unwrap(),
            },
            power,
        }
    }

    #[test]
    fn test_parse_validator_public_key_valid() {
        // Known valid secp256k1 point (generator point G)
        let x = U256::from_str_radix(
            "79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798",
            16,
        )
        .unwrap();
        let y = U256::from_str_radix(
            "483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8",
            16,
        )
        .unwrap();

        let result = parse_validator_public_key(&x, &y);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_validator_public_key_invalid_point() {
        // Invalid point (not on the curve)
        let x = U256::from(1u64);
        let y = U256::from(1u64);

        let result = parse_validator_public_key(&x, &y);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_validators_empty() {
        let result = parse_validators(vec![]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_validators_single() {
        // Use the secp256k1 generator point
        let info = make_validator_info(
            "79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798",
            "483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8",
            100,
        );

        let result = parse_validators(vec![info]);
        assert!(result.is_ok());

        let validators = result.unwrap();
        assert_eq!(validators.len(), 1);
        assert_eq!(validators[0].voting_power, 100);
    }

    #[test]
    fn test_parse_validators_multiple() {
        // Generator point G
        let info1 = make_validator_info(
            "79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798",
            "483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8",
            100,
        );

        // 2G (double of generator) - correct y coordinate
        let info2 = make_validator_info(
            "C6047F9441ED7D6D3045406E95C07CD85C778E4B8CEF3CA7ABAC09B95C709EE5",
            "E51E970159C23CC65C3A7BE6B99315110809CD9ACD992F1EDC9BCE55AF301705",
            200,
        );

        let result = parse_validators(vec![info1, info2]);
        assert!(result.is_ok(), "Failed: {:?}", result.err());

        let validators = result.unwrap();
        assert_eq!(validators.len(), 2);
        assert_eq!(validators[0].voting_power, 100);
        assert_eq!(validators[1].voting_power, 200);
    }

    #[test]
    fn test_parse_validators_with_invalid_key_fails() {
        let valid_info = make_validator_info(
            "79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798",
            "483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8",
            100,
        );

        // Invalid point
        let invalid_info = make_validator_info("1", "1", 50);

        // Should fail because one validator has invalid key
        let result = parse_validators(vec![valid_info, invalid_info]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_validators_zero_power() {
        let info = make_validator_info(
            "79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798",
            "483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8",
            0,
        );

        let result = parse_validators(vec![info]);
        assert!(result.is_ok());

        let validators = result.unwrap();
        assert_eq!(validators.len(), 1);
        assert_eq!(validators[0].voting_power, 0);
    }
}
