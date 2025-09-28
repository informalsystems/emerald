//! Types for validator set management

use std::collections::{HashMap, HashSet};

use alloy_primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};

/// Represents a validator with their voting power and Ed25519 key
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Validator {
    /// Validator's address
    pub address: Address,
    /// Validator's Ed25519 public key for consensus
    pub ed25519_key: B256,
    /// Validator's voting power
    pub power: U256,
}

impl Validator {
    /// Create a new validator with Ed25519 key
    pub fn new_with_key(address: Address, ed25519_key: B256, power: U256) -> Self {
        Self {
            address,
            ed25519_key,
            power,
        }
    }

    /// Create a new validator without Ed25519 key (for backward compatibility)
    pub fn new(address: Address, power: U256) -> Self {
        Self {
            address,
            ed25519_key: B256::ZERO,
            power,
        }
    }
}

/// Complete validator set state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorSet {
    /// Map of validator addresses to their data
    pub validators: HashMap<Address, Validator>,
    /// Set of validator addresses for enumeration
    pub validator_addresses: HashSet<Address>,
    /// Ordered list of validator addresses reflecting registration order
    pub validator_order: Vec<Address>,
    /// Total power of all validators
    pub total_power: U256,
}

impl ValidatorSet {
    /// Create a new empty validator set
    pub fn new() -> Self {
        Self {
            validators: HashMap::new(),
            validator_addresses: HashSet::new(),
            validator_order: Vec::new(),
            total_power: U256::ZERO,
        }
    }

    /// Add a validator to the set
    pub fn add_validator(&mut self, validator: Validator) {
        if let Some(existing) = self.validators.get(&validator.address) {
            self.total_power = self
                .total_power
                .saturating_sub(existing.power)
                .saturating_add(validator.power);
        } else {
            self.total_power += validator.power;
            self.validator_addresses.insert(validator.address);
            self.validator_order.push(validator.address);
        }

        self.validators.insert(validator.address, validator);
    }

    /// Get the number of validators
    pub fn count(&self) -> usize {
        self.validators.len()
    }

    /// Get all validators as a vector
    pub fn get_validators(&self) -> Vec<&Validator> {
        self.validator_order
            .iter()
            .filter_map(|addr| self.validators.get(addr))
            .collect()
    }
}

impl Default for ValidatorSet {
    fn default() -> Self {
        Self::new()
    }
}
