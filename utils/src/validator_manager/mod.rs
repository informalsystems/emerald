//! # ValidatorSet Storage Generator
//!
//! This library provides functionality to generate storage slots and values
//! for the ValidatorSet smart contract based on a given validator list.

pub mod error;
pub mod storage;
#[cfg(test)]
mod tests;
pub mod types;

use std::collections::{BTreeMap, HashSet};

use alloy_primitives::{Address, Bytes, B256, U256};
pub use emerald_contracts::{
    ValidatorManager, ValidatorManagerProxy, GENESIS_VALIDATOR_MANAGER_ACCOUNT,
    GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT, UUPS_SELF_IMMUTABLE_LENGTH,
    UUPS_SELF_IMMUTABLE_OFFSETS,
};
pub use error::{Error as ValidatroManagerError, Result};
pub use storage::StorageSlotCalculator;
pub use types::{Validator, ValidatorKey, ValidatorSet};

/// Return ValidatorManager DEPLOYED_BYTECODE with the UUPS `__self` immutable
/// patched to `impl_address`. Required because genesis bypasses the constructor
/// which normally sets the immutable `__self = address(this)`.
pub fn patched_impl_bytecode(impl_address: Address) -> Bytes {
    let mut code = ValidatorManager::DEPLOYED_BYTECODE.to_vec();
    let padded = impl_address.into_word();
    for &offset in UUPS_SELF_IMMUTABLE_OFFSETS {
        assert!(
            code[offset..offset + UUPS_SELF_IMMUTABLE_LENGTH]
                .iter()
                .all(|&b| b == 0),
            "expected zero bytes at offset {offset}; UUPS_SELF_IMMUTABLE_OFFSETS may be stale"
        );
        code[offset..offset + UUPS_SELF_IMMUTABLE_LENGTH].copy_from_slice(padded.as_slice());
    }
    Bytes::from(code)
}

use crate::validator_manager::storage::{
    set_validator_addresses_set, set_validator_entries_mapping, EIP1967_IMPL_SLOT,
    INITIALIZABLE_SLOT, OWNABLE_SLOT, REENTRANCY_GUARD_SLOT,
};

/// Generate proxy storage slots and values for a given validator list.
///
/// The returned map is intended for the **proxy** account at `0x2000`.
/// It contains EIP-1967 + ERC-7201 slots plus the contract's own state.
pub fn generate_storage_data(
    validators: Vec<Validator>,
    owner: Address,
    impl_address: Address,
) -> Result<BTreeMap<B256, B256>> {
    // Validate validators
    if validators.is_empty() {
        return Err(ValidatroManagerError::EmptyValidatorSet);
    }

    for validator in &validators {
        if validator.power == 0 {
            let (x, y) = validator.validator_key;
            return Err(ValidatroManagerError::InvalidPower { x, y });
        }
    }

    // Check for duplicate validators by key
    let mut seen_keys = HashSet::new();
    for validator in &validators {
        let key = validator.validator_key;
        if !seen_keys.insert(key) {
            return Err(ValidatroManagerError::DuplicateValidator { x: key.0, y: key.1 });
        }
    }

    // Create validator set
    let mut validator_set = ValidatorSet::default();
    for validator in validators {
        validator_set.add_validator(validator)?;
    }

    // Generate storage data
    generate_from_validator_set(&validator_set, owner, impl_address)
}

/// Generate proxy storage from a validator set.
///
/// Storage layout (UUPS upgradeable, OZ 5.x with ERC-7201 namespaced storage):
///
///   EIP-1967 impl slot  : implementation address
///   ERC-7201 Ownable    : _owner
///   ERC-7201 ReentrancyGuard : _status = 1 (NOT_ENTERED)
///   ERC-7201 Initializable   : _initialized = 1, _initializing = false
///   Slot 0 : _validatorAddresses._inner._values  (EnumerableSet)
///   Slot 1 : _validatorAddresses._inner._positions
///   Slot 2 : _validators mapping(address => ValidatorInfo)
///   Slot 3 : _totalPower
pub fn generate_from_validator_set(
    validator_set: &ValidatorSet,
    owner: Address,
    impl_address: Address,
) -> Result<BTreeMap<B256, B256>> {
    let mut storage = BTreeMap::new();

    // -- EIP-1967: proxy implementation pointer --
    storage.insert(EIP1967_IMPL_SLOT, impl_address.into_word());

    // -- ERC-7201: OwnableUpgradeable._owner --
    storage.insert(OWNABLE_SLOT, owner.into_word());

    // -- ERC-7201: ReentrancyGuardUpgradeable._status = 1 (NOT_ENTERED) --
    storage.insert(
        REENTRANCY_GUARD_SLOT,
        B256::from(U256::from(1u64).to_be_bytes::<32>()),
    );

    // -- ERC-7201: Initializable._initialized = 1, _initializing = false --
    // Both fields are packed into a single slot: uint64 _initialized || bool _initializing
    // _initialized = 1 occupies the low 8 bytes; _initializing = false is already zero.
    storage.insert(
        INITIALIZABLE_SLOT,
        B256::from(U256::from(1u64).to_be_bytes::<32>()),
    );

    // -- Contract state: sequential slots starting at 0 --
    // Slot 0-1: _validatorAddresses (EnumerableSet, occupies 2 slots)
    set_validator_addresses_set(&mut storage, validator_set, U256::from(0))?;
    // Slot 2: _validators mapping
    set_validator_entries_mapping(&mut storage, validator_set, U256::from(2))?;
    // Slot 3: _totalPower
    let total_power_slot = B256::from(U256::from(3u64).to_be_bytes::<32>());
    let total_power = validator_set.total_power()?;
    storage.insert(
        total_power_slot,
        B256::from(U256::from(total_power).to_be_bytes::<32>()),
    );

    Ok(storage)
}

/// Generate storage for the **implementation** account at genesis.
///
/// The only thing needed is to disable initializers so nobody can call
/// `initialize()` on the bare implementation. This mimics what
/// `_disableInitializers()` does in the constructor (which doesn't run
/// during genesis alloc).
pub fn generate_impl_storage() -> BTreeMap<B256, B256> {
    let mut storage = BTreeMap::new();
    // _initialized = type(uint64).max = 0xffffffffffffffff
    storage.insert(
        INITIALIZABLE_SLOT,
        B256::from(U256::from(u64::MAX).to_be_bytes::<32>()),
    );
    storage
}
