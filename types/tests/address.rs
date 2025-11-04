use alloy_primitives::Address as AlloyAddress;
use malachitebft_eth_types::secp256k1::PublicKey;
use malachitebft_eth_types::Address;

#[test]
fn test_ethereum_address_derivation_anvil_account() {
    // Anvil test account #0
    // Private key: 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
    // Expected address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
    // Public key (uncompressed): 0x048318535b54105d4a7aae60c08fc45f9687181b4fdfc625bd1a753fa7397fed753547f11ca8696646f2f3acb08e31016afac23e630c5d11f59f61fef57b0d2aa5

    let public_key_hex = "048318535b54105d4a7aae60c08fc45f9687181b4fdfc625bd1a753fa7397fed753547f11ca8696646f2f3acb08e31016afac23e630c5d11f59f61fef57b0d2aa5";
    let pub_key_bytes = hex::decode(public_key_hex).unwrap();

    // Create PublicKey from uncompressed SEC1 bytes
    let public_key = PublicKey::from_sec1_bytes(&pub_key_bytes).unwrap();

    // Derive address
    let derived_address = Address::from_public_key(&public_key);

    // Expected Anvil address
    let expected = AlloyAddress::from([
        0xf3, 0x9f, 0xd6, 0xe5, 0x1a, 0xad, 0x88, 0xf6, 0xf4, 0xce, 0x6a, 0xb8, 0x82, 0x72, 0x79,
        0xcf, 0xff, 0xb9, 0x22, 0x66,
    ]);

    assert_eq!(
        derived_address.to_alloy_address(),
        expected,
        "Derived address {} doesn't match expected Anvil address {}",
        derived_address.to_alloy_address(),
        expected
    );
}
