//! # ValidatorSet Storage Generator
//!
//! This library provides functionality to generate storage slots and values
//! for the ValidatorSet smart contract based on a given validator list.

pub mod contract;
pub mod error;
pub mod storage;
#[cfg(test)]
mod tests;
pub mod types;

use std::collections::{BTreeMap, HashSet};

use alloy_primitives::{Address, B256, U256};
pub use error::{Result, StorageError};
pub use storage::StorageSlotCalculator;
pub use types::{Validator, ValidatorKey, ValidatorSet};

use crate::validator_manager::storage::{set_validator_entries_mapping, set_validator_keys_set};

/// Generate storage slots and values for a given validator list
pub fn generate_storage_data(
    validators: Vec<Validator>,
    owner: Address,
) -> Result<BTreeMap<B256, B256>> {
    // Validate validators
    if validators.is_empty() {
        return Err(StorageError::EmptyValidatorSet);
    }

    for validator in &validators {
        if validator.power == 0 {
            let (x, y) = validator.validator_key;
            return Err(StorageError::InvalidPower { x, y });
        }
    }

    // Check for duplicate validators by key
    let mut seen_keys = HashSet::new();
    for validator in &validators {
        let key = validator.validator_key;
        if !seen_keys.insert(key) {
            return Err(StorageError::DuplicateValidator { x: key.0, y: key.1 });
        }
    }

    // Create validator set
    let mut validator_set = ValidatorSet::default();
    for validator in validators {
        validator_set.add_validator(validator);
    }

    // Generate storage data
    generate_from_validator_set(&validator_set, owner)
}

/// Generate storage data from validator set
pub fn generate_from_validator_set(
    validator_set: &ValidatorSet,
    owner: Address,
) -> Result<BTreeMap<B256, B256>> {
    // Storage layout for ValidatorManager contract:
    // Slot 0: Ownable._owner (set separately by deployment or genesis tooling)
    // Slot 1: ReentrancyGuard._status (set to 1)
    // Slot 2: _validatorKeys._values (EnumerableSet internal storage)
    // Slot 3: _validatorKeys._positions
    // Slot 4: _validators mapping(bytes32 => ValidatorInfo)

    let mut storage = BTreeMap::new();

    // Ownable owner
    storage.insert(B256::ZERO, owner.into_word());

    // ReentrancyGuard initial status (_status = 1) at slot 1
    let status_slot = B256::from(U256::from(1u64).to_be_bytes::<32>());
    storage.insert(
        status_slot,
        B256::from(U256::from(1u64).to_be_bytes::<32>()),
    );

    set_validator_keys_set(&mut storage, validator_set, U256::from(2))?; // _validatorKeys base at slot 2
    set_validator_entries_mapping(&mut storage, validator_set, U256::from(4))?; // _validators mapping at slot 4

    Ok(storage)
}
