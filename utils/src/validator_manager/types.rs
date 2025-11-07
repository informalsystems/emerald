//! Types for validator set management

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use alloy_primitives::U256;

use crate::validator_manager::contract::ValidatorManager;
use crate::validator_manager::error::{Error as ValidatorManagerError, Result};

/// Tuple wrapper for an uncompressed secp256k1 public key (x, y limbs)
pub type ValidatorKey = (U256, U256);

/// In-memory representation of a validator entry
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validator {
    /// Tuple of 32-byte secp256k1 public key limbs `(x, y)` stored as U256 values
    pub validator_key: ValidatorKey,
    /// Voting power for the validator
    pub power: u64,
}

impl Validator {
    /// Construct a validator from the `(x, y)` limbs of an uncompressed secp256k1 public key and voting power
    pub fn from_public_key(secp256k1_key: ValidatorKey, power: u64) -> Self {
        Self {
            validator_key: secp256k1_key,
            power,
        }
    }
}

impl From<ValidatorManager::ValidatorInfo> for Validator {
    fn from(info: ValidatorManager::ValidatorInfo) -> Self {
        Self {
            validator_key: (info.validatorKey.x, info.validatorKey.y),
            power: info.power,
        }
    }
}

impl From<Validator> for ValidatorManager::ValidatorInfo {
    fn from(validator: Validator) -> Self {
        ValidatorManager::ValidatorInfo {
            validatorKey: ValidatorManager::Secp256k1Key {
                x: validator.validator_key.0,
                y: validator.validator_key.1,
            },
            power: validator.power,
        }
    }
}

/// Complete validator set state
#[derive(Debug, Clone, Default)]
pub struct ValidatorSet {
    /// Map of validator keys to their data
    validators: HashMap<ValidatorKey, Validator>,
    /// Ordered list of validator keys reflecting registration order
    validator_order: Vec<ValidatorKey>,
    /// Aggregate voting power across validators
    total_power: u64,
}

impl ValidatorSet {
    /// Add a validator to the set
    pub fn add_validator(&mut self, validator: Validator) -> Result<()> {
        let key = validator.validator_key;

        if let Entry::Occupied(mut e) = self.validators.entry(key) {
            e.insert(validator);
            return Ok(());
        }

        self.total_power = self
            .total_power
            .checked_add(validator.power)
            .ok_or(ValidatorManagerError::TotalPowerOverflow)?;
        self.validator_order.push(key);
        self.validators.insert(key, validator);
        Ok(())
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

    /// Compute the total voting power across all validators
    pub fn total_power(&self) -> Result<u64> {
        Ok(self.total_power)
    }

    /// Ordered validator keys, useful for deterministic storage layout
    pub fn ordered_validator_keys(&self) -> &[ValidatorKey] {
        &self.validator_order
    }
}
