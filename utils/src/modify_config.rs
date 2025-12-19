use std::fs;
use std::path::Path;

use color_eyre::eyre::{Context, ContextCompat, Result};

/// Apply custom node configurations from a TOML file to individual node config files
pub fn apply_custom_config(node_config_home: &Path, custom_config_file_path: &Path) -> Result<()> {
    // Read and parse the custom config file
    let custom_config_contents = fs::read_to_string(custom_config_file_path).context(format!(
        "Failed to read custom config file: {}",
        custom_config_file_path.display()
    ))?;

    let custom_config: toml::Table =
        toml::from_str(&custom_config_contents).context("Failed to parse custom config file")?;

    println!(
        "Reading custom config from: {}",
        custom_config_file_path.display()
    );
    println!("Node config home: {}", node_config_home.display());
    println!();

    let mut success_count = 0;

    // Process each node
    for node_key in custom_config.keys() {
        let node_num = node_key.split("node").last().unwrap().to_string();
        println!("Processing node {node_num}...");
        // Extract the node's custom configuration
        let node_custom_config = match extract_node_config(&custom_config, node_key) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("Error extracting config for node {node_key}: {e}");
                continue;
            }
        };

        // Apply the configuration to the node's config file
        match apply_config_to_node(node_config_home, node_num, &node_custom_config) {
            Ok(_) => {
                success_count += 1;
            }
            Err(e) => {
                eprintln!("Error applying config to node {node_key}: {e}");
            }
        }

        println!();
    }

    println!(
        "Successfully updated {success_count}/{} node configurations",
        custom_config.keys().len()
    );

    Ok(())
}

/// Extract configuration for a specific node from the custom config
fn extract_node_config(custom_config: &toml::Table, node_key: &str) -> Result<toml::Table> {
    let node_section = custom_config
        .get(node_key)
        .context(format!("Node section '{node_key}' not found"))?
        .as_table()
        .context(format!("Node section '{node_key}' is not a table"))?;

    let mut config = toml::Table::new();

    // Iterate through all keys in the node section
    for (key, value) in node_section.iter() {
        // Skip the 'ip' field as it's metadata, not config
        if key == "ip" {
            continue;
        }

        // Add all other fields to the extracted config
        config.insert(key.to_string(), value.clone());
    }

    Ok(config)
}

/// Apply custom configuration to a specific node's config.toml file
fn apply_config_to_node(
    node_config_home: &Path,
    node_num: String,
    custom_config: &toml::Table,
) -> Result<()> {
    let config_path = node_config_home
        .join(node_num.clone())
        .join("config")
        .join("config.toml");

    if !config_path.exists() {
        return Err(color_eyre::eyre::eyre!(
            "Config file not found at {}",
            config_path.display()
        ));
    }

    // Read existing config
    let existing_config_contents = fs::read_to_string(&config_path).context(format!(
        "Failed to read config file: {}",
        config_path.display()
    ))?;

    let mut existing_config: toml::Table = toml::from_str(&existing_config_contents)
        .context("Failed to parse existing config file")?;

    // Create backup
    let backup_path = config_path.with_extension("toml.bak");
    fs::copy(&config_path, &backup_path).context(format!(
        "Failed to create backup at {}",
        backup_path.display()
    ))?;

    // Merge custom config into existing config
    deep_merge(&mut existing_config, custom_config);

    // Write updated config back to file
    let updated_config_str =
        toml::to_string_pretty(&existing_config).context("Failed to serialize updated config")?;

    fs::write(&config_path, updated_config_str).context(format!(
        "Failed to write config file: {}",
        config_path.display()
    ))?;

    println!(
        "Updated config for node {} at {}",
        node_num,
        config_path.display()
    );

    Ok(())
}

/// Recursively merge `update` into `base`
fn deep_merge(base: &mut toml::Table, update: &toml::Table) {
    for (key, value) in update {
        if let Some(base_value) = base.get_mut(key) {
            // If both are tables, merge recursively
            if let (Some(base_table), Some(update_table)) =
                (base_value.as_table_mut(), value.as_table())
            {
                deep_merge(base_table, update_table);
                continue;
            }
        }

        // Otherwise, replace the value
        base.insert(key.clone(), value.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deep_merge() {
        let mut base: toml::Table = toml::from_str(
            r#"
            [consensus]
            enabled = true
            timeout = "3s"

            [consensus.p2p]
            listen_addr = "/ip4/127.0.0.1/tcp/27000"
            "#,
        )
        .unwrap();

        let update: toml::Table = toml::from_str(
            r#"
            [consensus.p2p]
            listen_addr = "/ip4/127.0.0.1/tcp/27001"
            persistent_peers = ["/ip4/127.0.0.1/tcp/27002"]
            "#,
        )
        .unwrap();

        deep_merge(&mut base, &update);

        let consensus = base.get("consensus").unwrap().as_table().unwrap();
        let p2p = consensus.get("p2p").unwrap().as_table().unwrap();

        assert_eq!(
            p2p.get("listen_addr").unwrap().as_str().unwrap(),
            "/ip4/127.0.0.1/tcp/27001"
        );
        assert!(p2p.get("persistent_peers").is_some());
    }
}
