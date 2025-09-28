//! # ValidatorSet Storage Generator
//!
//! This library provides functionality to generate storage slots and values
//! for the ValidatorSet smart contract based on a given validator list.

pub mod error;
pub mod storage;
pub mod types;

use std::collections::BTreeMap;

use alloy_primitives::{B256, U256};
pub use error::{Result, StorageError};
pub use storage::StorageSlotCalculator;
pub use types::{Validator, ValidatorSet};

use crate::validator_set::storage::{set_validator_addresses_set, set_validators_mapping};

/// Generate storage slots and values for a given validator list
pub fn generate_storage_data(validators: Vec<Validator>) -> Result<BTreeMap<B256, B256>> {
    // Validate validators
    if validators.is_empty() {
        return Err(StorageError::EmptyValidatorSet);
    }

    for validator in &validators {
        if validator.power == U256::ZERO {
            return Err(StorageError::InvalidPower(validator.address));
        }
    }

    // Check for duplicate validators
    let mut seen_addresses = std::collections::HashSet::new();
    for validator in &validators {
        if !seen_addresses.insert(validator.address) {
            return Err(StorageError::DuplicateValidator(validator.address));
        }
    }

    // Create validator set
    let mut validator_set = ValidatorSet::new();
    for validator in validators {
        validator_set.add_validator(validator);
    }

    // Generate storage data
    generate_from_validator_set(&validator_set)
}

/// Generate storage data from validator set
pub fn generate_from_validator_set(validator_set: &ValidatorSet) -> Result<BTreeMap<B256, B256>> {
    // Storage layout for ValidatorSet contract:
    // Slot 0: ReentrancyGuard._status (set to 1)
    // Slot 1: _validatorAddresses (EnumerableSet.AddressSet)
    // Slot 3: _validators mapping(address => ValidatorInfo)

    let mut storage = BTreeMap::new();

    // ReentrancyGuard initial status
    storage.insert(B256::ZERO, B256::from(U256::from(1u64))); // _status = 1

    set_validator_addresses_set(&mut storage, validator_set)?;
    set_validators_mapping(&mut storage, validator_set)?;

    Ok(storage)
}
