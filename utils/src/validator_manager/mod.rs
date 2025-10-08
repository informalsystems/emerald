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

use std::collections::BTreeMap;

use alloy_primitives::{Address, B256, U256};
pub use error::{Result, StorageError};
pub use storage::StorageSlotCalculator;
pub use types::{Validator, ValidatorSet};

use crate::validator_manager::storage::{set_validator_keys_set, set_validator_powers_mapping};

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
        if validator.power == U256::ZERO {
            return Err(StorageError::InvalidPower(validator.validatorKey));
        }
    }

    // Check for duplicate validators by key
    let mut seen_keys = std::collections::HashSet::new();
    for validator in &validators {
        if !seen_keys.insert(validator.validatorKey) {
            return Err(StorageError::DuplicateValidator(validator.validatorKey));
        }
    }

    // Create validator set
    let mut validator_set = ValidatorSet::new();
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
    // Slot 2: _validatorKeys (EnumerableSet.UintSet)
    // Slot 3: (unused / future expansion)
    // Slot 4: _validatorPowers mapping(uint256 => uint256)

    let mut storage = BTreeMap::new();

    // Ownable owner
    storage.insert(B256::ZERO, owner.into_word());

    // ReentrancyGuard initial status (_status = 1) at slot 1
    let status_slot = B256::from(U256::from(1u64).to_be_bytes::<32>());
    storage.insert(
        status_slot,
        B256::from(U256::from(1u64).to_be_bytes::<32>()),
    );

    set_validator_keys_set(&mut storage, validator_set, U256::from(2))?; // _validatorKeys at slot 2
    set_validator_powers_mapping(&mut storage, validator_set, U256::from(4))?; // _validatorPowers mapping at slot 4

    Ok(storage)
}
