use malachitebft_signing_ed25519::PrivateKey;
use std::path::PathBuf;

use clap::Args;
use color_eyre::eyre::{Context, Result};

/// Extract and display Ed25519 public key from Tendermint private key file
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

        // Get the public key
        let public_key = private_key.public_key();

        // Output as hex
        println!("0x{}", hex::encode(public_key.as_bytes()));

        Ok(())
    }
}
