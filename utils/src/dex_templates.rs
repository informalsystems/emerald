use std::fs;
use std::path::Path;

use alloy_primitives::{Address, Bytes, U256};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};

/// Root structure for deserializing transaction templates from YAML
#[derive(Debug, Deserialize, Serialize)]
pub struct TransactionTemplates {
    pub transactions: Vec<TxTemplate>,
}

/// Enum representing different transaction types
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TxTemplate {
    Eip1559(Eip1559Template),
    Eip4844(Eip4844Template),
}

/// Template for EIP-1559 transactions
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Eip1559Template {
    /// Recipient address
    pub to: String,
    /// Value to send in ETH (as string, e.g., "0.001")
    pub value: String,
    /// Gas limit
    pub gas_limit: u64,
    /// Maximum fee per gas in gwei (as string, e.g., "2")
    pub max_fee_per_gas: String,
    /// Maximum priority fee per gas in gwei (as string, e.g., "1")
    pub max_priority_fee_per_gas: String,
    /// Optional hex-encoded input data (e.g., "0xabcd1234")
    #[serde(default)]
    pub input: String,
}

/// Template for EIP-4844 (blob) transactions
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Eip4844Template {
    /// Recipient address
    pub to: String,
    /// Value to send in ETH (as string, e.g., "0.001")
    pub value: String,
    /// Gas limit
    pub gas_limit: u64,
    /// Maximum fee per gas in gwei (as string, e.g., "50")
    pub max_fee_per_gas: String,
    /// Maximum priority fee per gas in gwei (as string, e.g., "1000")
    pub max_priority_fee_per_gas: String,
    /// Maximum fee per blob gas in gwei (as string, e.g., "1")
    pub max_fee_per_blob_gas: String,
}

/// Parse ETH value from string (e.g., "0.001" -> Wei)
pub fn parse_eth_value(eth: &str) -> Result<U256> {
    let value: f64 = eth
        .parse()
        .map_err(|e| eyre!("Failed to parse ETH value '{}': {}", eth, e))?;

    if value < 0.0 {
        return Err(eyre!("ETH value cannot be negative: {}", eth));
    }

    // Convert ETH to Wei (1 ETH = 10^18 Wei)
    let wei = (value * 1e18) as u128;
    Ok(U256::from(wei))
}

/// Parse gwei value from string (e.g., "2" -> Wei)
pub fn parse_gwei_value(gwei: &str) -> Result<u128> {
    let value: f64 = gwei
        .parse()
        .map_err(|e| eyre!("Failed to parse gwei value '{}': {}", gwei, e))?;

    if value < 0.0 {
        return Err(eyre!("Gwei value cannot be negative: {}", gwei));
    }

    // Convert gwei to Wei (1 gwei = 10^9 Wei)
    Ok((value * 1e9) as u128)
}

/// Parse hex-encoded address from string
pub fn parse_address(addr: &str) -> Result<Address> {
    addr.parse::<Address>()
        .map_err(|e| eyre!("Failed to parse address '{}': {}", addr, e))
}

/// Parse hex-encoded input data from string
pub fn parse_input_data(input: &str) -> Result<Bytes> {
    if input.is_empty() {
        return Ok(Bytes::default());
    }

    let hex_str = input.trim_start_matches("0x");
    let bytes =
        hex::decode(hex_str).map_err(|e| eyre!("Failed to parse input data '{}': {}", input, e))?;

    Ok(Bytes::from(bytes))
}

/// Load transaction templates from a YAML file
pub fn load_templates(path: impl AsRef<Path>) -> Result<Vec<TxTemplate>> {
    let path_ref = path.as_ref();
    let content = fs::read_to_string(path_ref).map_err(|e| {
        eyre!(
            "Failed to read template file '{}': {}",
            path_ref.display(),
            e
        )
    })?;

    let templates: TransactionTemplates = serde_yaml::from_str(&content)
        .map_err(|e| eyre!("Failed to parse YAML from '{}': {}", path_ref.display(), e))?;

    if templates.transactions.is_empty() {
        return Err(eyre!(
            "Template file '{}' contains no transactions",
            path_ref.display()
        ));
    }

    Ok(templates.transactions)
}

/// Save transaction templates to a YAML file
pub fn save_templates(templates: &[TxTemplate], path: impl AsRef<Path>) -> Result<()> {
    let path_ref = path.as_ref();

    // Create parent directory if it doesn't exist
    if let Some(parent) = path_ref.parent() {
        fs::create_dir_all(parent)?;
    }

    let wrapper = TransactionTemplates {
        transactions: templates.to_vec(),
    };

    let yaml_content = serde_yaml::to_string(&wrapper)
        .map_err(|e| eyre!("Failed to serialize templates to YAML: {}", e))?;

    fs::write(path_ref, yaml_content).map_err(|e| {
        eyre!(
            "Failed to write template file '{}': {}",
            path_ref.display(),
            e
        )
    })?;

    Ok(())
}

/// Round-robin selector for cycling through transaction templates
pub struct RoundRobinSelector {
    templates: Vec<TxTemplate>,
    current_index: usize,
}

impl RoundRobinSelector {
    /// Create a new round-robin selector with the given templates
    pub fn new(templates: Vec<TxTemplate>) -> Self {
        Self {
            templates,
            current_index: 0,
        }
    }

    /// Get the next template in round-robin fashion
    pub fn next_template(&mut self) -> &TxTemplate {
        let template = &self.templates[self.current_index];
        self.current_index = (self.current_index + 1) % self.templates.len();
        template
    }

    /// Reset the selector back to the first template
    /// Used when nonce mismatches occur to restart the template sequence
    pub fn reset(&mut self) {
        self.current_index = 0;
    }

    /// Set the selector to a specific template index
    /// Used for smart nonce recovery to resume at the correct position in the template cycle
    pub fn set_index(&mut self, index: usize) {
        self.current_index = index % self.templates.len();
    }

    /// Get the number of templates in the cycle
    pub fn template_count(&self) -> usize {
        self.templates.len()
    }

    /// Get the current template index
    pub fn current_index(&self) -> usize {
        self.current_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_eth_value() {
        assert_eq!(
            parse_eth_value("1").unwrap(),
            U256::from(1_000_000_000_000_000_000u128)
        );
        assert_eq!(
            parse_eth_value("0.001").unwrap(),
            U256::from(1_000_000_000_000_000u128)
        );
        assert_eq!(parse_eth_value("0").unwrap(), U256::ZERO);
    }

    #[test]
    fn test_parse_eth_value_invalid() {
        assert!(parse_eth_value("invalid").is_err());
        assert!(parse_eth_value("-1").is_err());
    }

    #[test]
    fn test_parse_gwei_value() {
        assert_eq!(parse_gwei_value("1").unwrap(), 1_000_000_000u128);
        assert_eq!(parse_gwei_value("2.5").unwrap(), 2_500_000_000u128);
        assert_eq!(parse_gwei_value("0").unwrap(), 0u128);
    }

    #[test]
    fn test_parse_gwei_value_invalid() {
        assert!(parse_gwei_value("invalid").is_err());
        assert!(parse_gwei_value("-1").is_err());
    }

    #[test]
    fn test_parse_address() {
        let addr = "0x0000000000000000000000000000000000000005";
        assert!(parse_address(addr).is_ok());
    }

    #[test]
    fn test_parse_address_invalid() {
        assert!(parse_address("invalid").is_err());
        assert!(parse_address("0x123").is_err()); // Too short
    }

    #[test]
    fn test_parse_input_data() {
        assert_eq!(parse_input_data("").unwrap(), Bytes::default());
        assert_eq!(parse_input_data("0x").unwrap(), Bytes::default());

        let data = parse_input_data("0xabcd").unwrap();
        assert_eq!(data.len(), 2);
    }

    #[test]
    fn test_parse_input_data_invalid() {
        assert!(parse_input_data("0xGGGG").is_err()); // Invalid hex
    }

    #[test]
    fn test_round_robin_selector() {
        let templates = vec![
            TxTemplate::Eip1559(Eip1559Template {
                to: "0x0000000000000000000000000000000000000001".to_string(),
                value: "0.001".to_string(),
                gas_limit: 21000,
                max_fee_per_gas: "2".to_string(),
                max_priority_fee_per_gas: "1".to_string(),
                input: "".to_string(),
            }),
            TxTemplate::Eip1559(Eip1559Template {
                to: "0x0000000000000000000000000000000000000002".to_string(),
                value: "0.002".to_string(),
                gas_limit: 21000,
                max_fee_per_gas: "3".to_string(),
                max_priority_fee_per_gas: "1.5".to_string(),
                input: "".to_string(),
            }),
        ];

        let mut selector = RoundRobinSelector::new(templates);

        // First iteration
        if let TxTemplate::Eip1559(t) = selector.next_template() {
            assert_eq!(t.to, "0x0000000000000000000000000000000000000001");
        }

        // Second iteration
        if let TxTemplate::Eip1559(t) = selector.next_template() {
            assert_eq!(t.to, "0x0000000000000000000000000000000000000002");
        }

        // Should wrap around
        if let TxTemplate::Eip1559(t) = selector.next_template() {
            assert_eq!(t.to, "0x0000000000000000000000000000000000000001");
        }
    }
}
