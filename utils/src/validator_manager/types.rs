//! Types for validator set management

use std::collections::{HashMap, HashSet};

use alloy_primitives::{B256, U256};

use crate::validator_manager::contract::ValidatorManager;

/// Direct alias to the Solidity-generated validator info struct
pub type Validator = ValidatorManager::ValidatorInfo;

impl ValidatorManager::ValidatorInfo {
    /// Construct a validator from a raw Ed25519 key encoded as `B256`
    pub fn from_public_key(ed25519_key: B256, power: U256) -> Self {
        Self {
            validatorKey: U256::from_be_slice(ed25519_key.as_slice()),
            power,
        }
    }
}

/// Complete validator set state
#[derive(Debug, Clone)]
pub struct ValidatorSet {
    /// Map of validator keys to their data
    pub validators: HashMap<U256, Validator>,
    /// Set of validator keys for enumeration
    pub validator_keys: HashSet<U256>,
    /// Ordered list of validator keys reflecting registration order
    pub validator_order: Vec<U256>,
    /// Total power of all validators
    pub total_power: U256,
}

impl ValidatorSet {
    /// Create a new empty validator set
    pub fn new() -> Self {
        Self {
            validators: HashMap::new(),
            validator_keys: HashSet::new(),
            validator_order: Vec::new(),
            total_power: U256::ZERO,
        }
    }

    /// Add a validator to the set
    pub fn add_validator(&mut self, validator: Validator) {
        if let Some(existing) = self.validators.get(&validator.validatorKey) {
            self.total_power = self
                .total_power
                .saturating_sub(existing.power)
                .saturating_add(validator.power);
        } else {
            self.total_power += validator.power;
            self.validator_keys.insert(validator.validatorKey);
            self.validator_order.push(validator.validatorKey);
        }

        self.validators.insert(validator.validatorKey, validator);
    }

    /// Get the number of validators
    pub fn count(&self) -> usize {
        self.validators.len()
    }

    /// Get all validators as a vector
    pub fn get_validators(&self) -> Vec<&Validator> {
        self.validator_order
            .iter()
            .filter_map(|key| self.validators.get(key))
            .collect()
    }
}

impl Default for ValidatorSet {
    fn default() -> Self {
        Self::new()
    }
}
