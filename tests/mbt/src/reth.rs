use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

const MAX_STARTUP_ATTEMPTS: u32 = 30;
const STARTUP_CHECK_INTERVAL: Duration = Duration::from_secs(2);

pub struct RethManager {
    process: Option<Child>,
    data_dir: PathBuf,
}

impl RethManager {
    /// Start a new RETH instance for testing
    pub fn start() -> anyhow::Result<Self> {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()?;

        let data_dir = project_root.join("tests/mbt/.reth-data");
        let genesis_file = project_root.join("assets/genesis.json");
        let emerald_genesis_file = project_root.join("assets/emerald_genesis.json");
        let pubkeys_file = project_root.join("assets/validator_public_keys.txt");
        let jwt_secret = project_root.join("assets/jwtsecret");

        // Clean up old data directory for a fresh start
        if data_dir.exists() {
            std::fs::remove_dir_all(&data_dir)?;
        }
        std::fs::create_dir_all(&data_dir)?;

        // Ensure JWT secret exists
        if !jwt_secret.exists() {
            if let Some(parent) = jwt_secret.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let output = Command::new("openssl")
                .args(["rand", "-hex", "32"])
                .output()?;
            std::fs::write(&jwt_secret, output.stdout)?;
        }

        // Generate genesis if it doesn't exist
        if !genesis_file.exists() {
            println!("Generating genesis files...");

            // Clean up related files to ensure consistency
            let _ = std::fs::remove_file(&emerald_genesis_file);
            let _ = std::fs::remove_file(&pubkeys_file);

            // Generate validator keys
            let mbt_dir = project_root.join("tests/mbt");
            let output = Command::new("cargo")
                .args(["run", "--bin", "generate-validator-keys"])
                .current_dir(&mbt_dir)
                .output()?;

            if !output.status.success() {
                anyhow::bail!(
                    "Failed to generate validator keys: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            std::fs::write(&pubkeys_file, output.stdout)?;

            // Generate genesis using emerald-utils
            let emerald_utils = Self::find_binary(&project_root, "emerald-utils")?;
            let status = Command::new(emerald_utils)
                .args([
                    "genesis",
                    "--public-keys-file",
                    pubkeys_file.to_str().unwrap(),
                    "--poa-owner-address",
                    "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
                ])
                .current_dir(&project_root)
                .status()?;

            if !status.success() {
                anyhow::bail!("Failed to generate genesis files");
            }

            println!("✓ Genesis files generated");
        }

        // Find custom-reth binary
        let reth_bin = Self::find_binary(&project_root, "custom-reth")?;
        println!("Using reth binary: {}", reth_bin.display());

        // Start RETH process
        println!("Starting RETH...");
        let child = Command::new(reth_bin)
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
            .spawn()?;

        let manager = RethManager {
            process: Some(child),
            data_dir,
        };

        // Wait for RETH to be ready
        manager.wait_for_ready()?;
        println!("✓ RETH is running and ready");

        Ok(manager)
    }

    /// Find a binary in various possible locations
    fn find_binary(project_root: &Path, name: &str) -> anyhow::Result<PathBuf> {
        let possible_paths = if name == "custom-reth" {
            vec![
                project_root.join(format!("custom-reth/target/release/{}", name)),
                project_root.join(format!("custom-reth/target/debug/{}", name)),
                project_root.join(format!("target/release/{}", name)),
                project_root.join(format!("target/debug/{}", name)),
            ]
        } else {
            vec![
                project_root.join(format!("target/release/{}", name)),
                project_root.join(format!("target/debug/{}", name)),
            ]
        };

        for path in possible_paths {
            if path.exists() {
                return Ok(path);
            }
        }

        anyhow::bail!("Binary '{}' not found. Please build it first.", name)
    }

    /// Check if RETH is responding to RPC requests
    fn check_reth_ready() -> bool {
        match ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(1))
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

    /// Wait for RETH to be ready to accept connections
    fn wait_for_ready(&self) -> anyhow::Result<()> {
        for attempt in 1..=MAX_STARTUP_ATTEMPTS {
            if Self::check_reth_ready() {
                return Ok(());
            }

            println!(
                "Waiting for RETH to start ({}/{})",
                attempt, MAX_STARTUP_ATTEMPTS
            );
            thread::sleep(STARTUP_CHECK_INTERVAL);
        }

        anyhow::bail!(
            "RETH failed to start after {} attempts",
            MAX_STARTUP_ATTEMPTS
        )
    }

    /// Stop the RETH process
    pub fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(mut child) = self.process.take() {
            println!("Shutting down RETH...");

            // Try graceful shutdown first
            child.kill()?;

            // Wait a moment for graceful shutdown
            thread::sleep(Duration::from_secs(2));

            // Ensure it's dead
            let _ = child.wait();

            println!("✓ RETH stopped");
        }

        Ok(())
    }
}

impl Drop for RethManager {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
