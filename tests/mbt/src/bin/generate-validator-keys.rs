use k256::ecdsa::VerifyingKey;
use malachitebft_eth_types::utils::validators::make_validators_with_individual_seeds;

fn main() {
    // Generate 3 validators with equal voting power (1 each)
    // Using individual seeds: 0, 1, 2
    let validators = make_validators_with_individual_seeds([1, 1, 1]);

    // Output public keys in uncompressed hex format (0x + 128 hex chars)
    // This matches the format expected by emerald-utils genesis command
    for (validator, _private_key) in &validators {
        let pub_key = &validator.public_key;

        // Get compressed bytes from public key
        let compressed_bytes = pub_key.to_vec();

        // Parse to get VerifyingKey and convert to uncompressed
        let verifying_key = VerifyingKey::from_sec1_bytes(&compressed_bytes)
            .expect("PublicKey to_vec() should always return valid SEC1 bytes");

        // Get uncompressed point (65 bytes: 0x04 || x || y)
        let uncompressed_point = verifying_key.to_encoded_point(false);
        let uncompressed_bytes = uncompressed_point.as_bytes();

        // Skip the 0x04 prefix byte (first byte) and encode the x and y coordinates
        let hex_str = hex::encode(&uncompressed_bytes[1..]);

        println!("0x{}", hex_str);
    }
}
