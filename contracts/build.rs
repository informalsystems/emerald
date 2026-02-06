use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");

    println!("cargo::rerun-if-changed=../solidity/src");
    println!("cargo::rerun-if-changed=../foundry.toml");

    let status = Command::new("forge")
        .args(["build", "--skip", "test", "script"])
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

    let compiled_contracts = ["ValidatorManager"];

    for name in compiled_contracts {
        let artifact = workspace_root.join(format!("solidity/out/{name}.sol/{name}.json"));
        assert!(
            fs::metadata(&artifact).is_ok(),
            "expected artifact not found: {}",
            artifact.display()
        );
    }
}
