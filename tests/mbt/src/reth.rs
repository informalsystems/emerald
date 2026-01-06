use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use k256::ecdsa::VerifyingKey;
use malachitebft_app_channel::app::types::core::VotingPower;
use malachitebft_eth_types::utils::validators::make_validators_with_individual_seeds;

use crate::N_NODES;

const MAX_STARTUP_ATTEMPTS: u32 = 30;
const STARTUP_CHECK_INTERVAL: Duration = Duration::from_secs(2);
const CHECK_READY_TIMEOUT: Duration = Duration::from_secs(1);

/// A handle that holds a started Reth child process. This handle must be held
/// for the duration of the test run. Dropping the handle shuts down the Reth
/// process.
pub struct RethHandle(Child);

impl Drop for RethHandle {
    fn drop(&mut self) {
        self.0.kill().expect("Failed to SIGKILL reth");
        self.0.wait().expect("Failed to wait for reth to die");
    }
}

pub fn start() -> Result<RethHandle> {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;

    let data_dir = project_root.join("tests/mbt/.reth-data");
    let assets_dir = project_root.join("assets");
    let genesis_file = assets_dir.join("genesis.json");
    let pubkeys_file = assets_dir.join("validator_public_keys.txt");
    let jwt_secret = assets_dir.join("jwtsecret");

    if data_dir.exists() {
        // Clean up old data for a fresh start
        std::fs::remove_dir_all(&data_dir)?;
    }
    std::fs::create_dir_all(&data_dir)?;
    std::fs::create_dir_all(&assets_dir)?;

    ensure_jwt_secret(&jwt_secret)?;
    ensure_genesis_files(&project_root, &genesis_file, &pubkeys_file)?;
    start_reth(&project_root, &data_dir, &genesis_file, &jwt_secret)
}

fn ensure_jwt_secret(jwt_secret: &Path) -> Result<()> {
    if !jwt_secret.exists() {
        let output = Command::new("openssl")
            .args(["rand", "-hex", "32"])
            .output()?;
        std::fs::write(jwt_secret, output.stdout)?;
    }
    Ok(())
}

fn ensure_genesis_files(
    project_root: &Path,
    genesis_file: &Path,
    pubkeys_file: &Path,
) -> Result<()> {
    if !genesis_file.exists() {
        println!("Generating genesis files...");
        generate_validator_keys(pubkeys_file)?;
        generate_genesis_files(project_root, pubkeys_file)?;
    }
    Ok(())
}

fn generate_validator_keys(pubkeys_file: &Path) -> Result<()> {
    // Generate N_NODES validators with equal voting power (1 each)
    // Using individual seeds: 0, 1, 2, ...
    let voting_powers = std::array::repeat::<VotingPower, N_NODES>(1);
    let validators = make_validators_with_individual_seeds(voting_powers);

    // Public keys in uncompressed hex format (0x + 128 hex chars)
    // This matches the format expected by emerald-utils genesis command
    let mut public_keys = String::new();

    for (validator, _private_key) in &validators {
        let pub_key = &validator.public_key;
        let compressed_bytes = pub_key.to_vec();
        let verifying_key = VerifyingKey::from_sec1_bytes(&compressed_bytes)
            .expect("PublicKey to_vec() should always return valid SEC1 bytes");

        let uncompressed_point = verifying_key.to_encoded_point(false);
        let uncompressed_bytes = uncompressed_point.as_bytes();
        let hex_str = hex::encode(&uncompressed_bytes[1..]);

        public_keys.push_str("0x");
        public_keys.push_str(&hex_str);
        public_keys.push('\n');
    }

    std::fs::write(pubkeys_file, public_keys)?;
    Ok(())
}

fn generate_genesis_files(project_root: &Path, pubkeys_file: &Path) -> Result<()> {
    let status = Command::new("target/debug/emerald-utils")
        .args([
            "genesis",
            "--public-keys-file",
            pubkeys_file.to_str().unwrap(),
            "--poa-owner-address",
            "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
        ])
        .current_dir(project_root)
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to generate genesis files");
    }

    Ok(())
}

fn start_reth(
    project_root: &Path,
    data_dir: &Path,
    genesis_file: &Path,
    jwt_secret: &Path,
) -> Result<RethHandle> {
    println!("Starting RETH...");
    let child = Command::new("custom-reth/target/debug/custom-reth")
        .args([
            "node",
            "--chain",
            genesis_file.to_str().unwrap(),
            "--dev",
            "--dev.block-time=1s",
            "--http",
            "--http.addr=127.0.0.1",
            "--http.port=8545",
            "--http.api=eth,net,web3,debug,txpool,trace",
            "--http.corsdomain=*",
            "--authrpc.addr=127.0.0.1",
            "--authrpc.port=8551",
            "--authrpc.jwtsecret",
            jwt_secret.to_str().unwrap(),
            "--datadir",
            data_dir.to_str().unwrap(),
            "--log.stdout.filter=debug",
        ])
        .current_dir(project_root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    wait_for_reth()?;

    Ok(RethHandle(child))
}

fn wait_for_reth() -> Result<()> {
    for attempt in 1..=MAX_STARTUP_ATTEMPTS {
        thread::sleep(STARTUP_CHECK_INTERVAL);
        if check_reth_ready() {
            return Ok(());
        }

        println!(
            "Waiting for RETH to start ({}/{})",
            attempt, MAX_STARTUP_ATTEMPTS
        );
    }

    anyhow::bail!(
        "RETH failed to start after {} attempts",
        MAX_STARTUP_ATTEMPTS
    )
}

fn check_reth_ready() -> bool {
    match ureq::AgentBuilder::new()
        .timeout(CHECK_READY_TIMEOUT)
        .build()
        .post("http://localhost:8545")
        .send_json(ureq::json!({
            "jsonrpc": "2.0",
            "method": "eth_blockNumber",
            "params": [],
            "id": 1
        })) {
        Ok(response) => response.status() == 200,
        Err(_) => false,
    }
}
