use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;

fn main() {
    // Compute workspace root
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .expect("Failed to canonicalize workspace root");

    // Define paths
    let solidity_src = workspace_root.join("solidity/src");
    let foundry_toml = workspace_root.join("solidity/foundry.toml");
    let artifact_path =
        workspace_root.join("solidity/out/ValidatorManager.sol/ValidatorManager.json");

    // Emit rerun-if-changed for all relevant files
    println!("cargo:rerun-if-changed=../solidity/src/ValidatorManager.sol");
    println!("cargo:rerun-if-changed=../solidity/foundry.toml");
    println!("cargo:rerun-if-changed=../solidity/out/ValidatorManager.sol/ValidatorManager.json");

    // Check if artifact exists and compare timestamps
    let needs_rebuild = if !artifact_path.exists() {
        true
    } else {
        // Get artifact modification time
        let artifact_mtime = std::fs::metadata(&artifact_path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        // Check if any Solidity source is newer than the artifact
        let mut sources_newer = false;
        if let Ok(entries) = std::fs::read_dir(&solidity_src) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "sol" {
                        if let Ok(metadata) = entry.metadata() {
                            if let Ok(mtime) = metadata.modified() {
                                if mtime > artifact_mtime {
                                    sources_newer = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Also check foundry.toml
        if let Ok(metadata) = std::fs::metadata(&foundry_toml) {
            if let Ok(mtime) = metadata.modified() {
                if mtime > artifact_mtime {
                    sources_newer = true;
                }
            }
        }

        sources_newer
    };

    // Run forge build if needed
    if needs_rebuild {
        println!("cargo:warning=Solidity artifacts missing or outdated, running forge build...");

        let status = Command::new("forge")
            .arg("build")
            .current_dir(&workspace_root)
            .status();

        match status {
            Ok(exit_status) if exit_status.success() => {
                println!("cargo:warning=forge build completed successfully");
            }
            Ok(exit_status) => {
                panic!(
                    "forge build failed with exit code: {:?}\nWorking directory: {}",
                    exit_status.code(),
                    workspace_root.display()
                );
            }
            Err(e) => {
                panic!(
                    "Failed to execute forge: {}\n\
                     Please install Foundry: https://getfoundry.sh/\n\
                     Working directory: {}",
                    e,
                    workspace_root.display()
                );
            }
        }
    }
}
