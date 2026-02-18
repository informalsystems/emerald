use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");

    println!("cargo::rerun-if-changed=../solidity/src/ValidatorManager.sol");
    println!(
        "cargo::rerun-if-changed=../solidity/lib/openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Proxy.sol"
    );
    println!("cargo::rerun-if-changed=../foundry.toml");
    println!("cargo::rerun-if-changed=../solidity/out/ValidatorManager.sol/ValidatorManager.json");
    println!("cargo::rerun-if-changed=../solidity/out/ERC1967Proxy.sol/ERC1967Proxy.json");

    let status = Command::new("forge")
        .args(["build", "--root", "solidity", "--skip", "test", "script"])
        .current_dir(&workspace_root)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => match s.code() {
            Some(code) => panic!("forge build failed with exit code {code}"),
            None => panic!("forge build terminated by signal"),
        },
        Err(e) => {
            panic!("failed to run forge: {e}\ninstall Foundry: https://getfoundry.sh/");
        }
    }

    let compiled_contracts = ["ValidatorManager", "ERC1967Proxy"];

    for name in compiled_contracts {
        let artifact = workspace_root.join(format!("solidity/out/{name}.sol/{name}.json"));
        assert!(
            fs::metadata(&artifact).is_ok(),
            "expected artifact not found: {}",
            artifact.display()
        );
    }
}
