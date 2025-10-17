use std::path::PathBuf;

use clap::Args;
use color_eyre::eyre::{ensure, Context, Result};
use malachitebft_eth_types::secp256k1::{PrivateKey, PublicKey};

/// Extract the validator's secp256k1 public key (without the 0x04 prefix) from a file containing a Secp256k1 private key
#[derive(Args, Clone, Debug)]
pub struct ShowPubkeyCmd {
    /// Path to priv_validator_key.json file
    #[clap(value_name = "KEY_FILE")]
    pub key_file: PathBuf,
}

impl ShowPubkeyCmd {
    pub fn run(&self) -> Result<()> {
        // Read and parse the JSON file
        let contents = std::fs::read_to_string(&self.key_file)
            .with_context(|| format!("Failed to read key file: {}", self.key_file.display()))?;

        // Deserialize directly into PrivateKey (which has Deserialize support)
        let private_key: PrivateKey = serde_json::from_str(&contents)
            .with_context(|| format!("Failed to parse JSON from: {}", self.key_file.display()))?;

        // Get the public key and output as uncompressed hex so callers can register it.
        let public_key: PublicKey = private_key.public_key();
        let uncompressed = public_key.to_uncompressed_bytes();

        ensure!(
            uncompressed.len() == 65,
            "expected uncompressed secp256k1 public key to be 65 bytes"
        );
        ensure!(
            uncompressed[0] == 0x04,
            "expected uncompressed secp256k1 public key to start with 0x04 prefix"
        );

        // Trim the leading 0x04 prefix so the caller receives the 64-byte (x || y) payload.
        let trimmed = &uncompressed[1..];
        println!("0x{}", hex::encode(trimmed));

        Ok(())
    }
}
