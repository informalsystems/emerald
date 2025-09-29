//! Storage layout and encoding for ValidatorSet contract

use std::collections::BTreeMap;

use alloy_primitives::{keccak256, Address, B256, U256};

use crate::validator_set::error::Result;
use crate::validator_set::types::ValidatorSet;

/// Storage slot calculator for Solidity mappings and arrays
pub struct StorageSlotCalculator;

impl StorageSlotCalculator {
    /// Calculate storage slot for mapping(key => value) where the key is encoded as bytes32
    pub fn mapping_slot(key: B256, base_slot: U256) -> B256 {
        let key_hash = keccak256([key.as_slice(), &base_slot.to_be_bytes::<32>()].concat());
        key_hash
    }

    /// Calculate storage slot for dynamic array element in `_validatorAddresses._inner._values`
    pub fn array_element_slot(base_slot: U256, index: U256) -> B256 {
        let array_base = keccak256(base_slot.to_be_bytes::<32>());
        let array_base_u256 = U256::from_be_slice(array_base.as_slice());
        let element_slot = array_base_u256 + index;
        B256::from(element_slot.to_be_bytes::<32>())
    }

    /// Calculate storage slot for a struct field at the given index (0-based)
    pub fn struct_field_slot(base_slot: B256, field_index: usize) -> B256 {
        let base = U256::from_be_slice(base_slot.as_slice());
        let field_slot = base + U256::from(field_index as u64);
        B256::from(field_slot.to_be_bytes::<32>())
    }
}

/// Set up the EnumerableSet for validator addresses
pub(crate) fn set_validator_addresses_set(
    storage: &mut BTreeMap<B256, B256>,
    validator_set: &ValidatorSet,
    base_slot: U256,
) -> Result<()> {
    let base_slot_b256 = B256::from(base_slot.to_be_bytes::<32>());
    let addresses: Vec<Address> = validator_set.validator_order.clone();

    // Slot 1 stores the length of the dynamic array `_inner._values`
    let length_slot = StorageSlotCalculator::struct_field_slot(base_slot_b256, 0);
    storage.insert(
        length_slot,
        B256::from(U256::from(addresses.len() as u64).to_be_bytes::<32>()),
    );

    // `_inner._positions` mapping is located at slot + 1
    let positions_base_slot_b256 = StorageSlotCalculator::struct_field_slot(base_slot_b256, 1);
    let positions_base_slot = U256::from_be_slice(positions_base_slot_b256.as_slice());

    for (index, &address) in addresses.iter().enumerate() {
        let address_word: B256 = address.into_word();

        // Write array element at base + index
        let element_slot =
            StorageSlotCalculator::array_element_slot(base_slot, U256::from(index as u64));
        storage.insert(element_slot, address_word);

        // Write mapping entry with 1-based index
        let position_slot = StorageSlotCalculator::mapping_slot(address_word, positions_base_slot);
        storage.insert(
            position_slot,
            B256::from(U256::from((index as u64) + 1).to_be_bytes::<32>()),
        );
    }

    Ok(())
}

/// Set up the validators mapping
pub(crate) fn set_validators_mapping(
    storage: &mut BTreeMap<B256, B256>,
    validator_set: &ValidatorSet,
    base_slot: U256,
) -> Result<()> {
    for validator in validator_set.get_validators() {
        let validator_slot =
            StorageSlotCalculator::mapping_slot(validator.address.into_word(), base_slot);

        // ValidatorInfo struct: { bytes32 ed25519Key; uint256 power; }
        // Slot 0 of struct: ed25519Key (bytes32)
        storage.insert(validator_slot, validator.ed25519_key);

        // Slot 1 of struct: power (uint256)
        let power_slot = StorageSlotCalculator::struct_field_slot(validator_slot, 1);
        storage.insert(power_slot, B256::from(validator.power.to_be_bytes::<32>()));
    }

    Ok(())
}
