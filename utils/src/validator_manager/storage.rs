//! Storage layout and encoding for the upgradeable ValidatorManager contract.
//!
//! The proxy (ERC1967) sits at `0x2000`; its storage contains:
//!   - EIP-1967 implementation slot pointing to the logic contract
//!   - ERC-7201 namespaced slots for OwnableUpgradeable, ReentrancyGuardUpgradeable,
//!     and Initializable
//!   - Sequential slots 0..3 for the contract's own state variables

use std::collections::BTreeMap;

use alloy_primitives::{hex, keccak256, Address, B256, U256};

use crate::validator_manager::error::Result;
use crate::validator_manager::types::{ValidatorKey, ValidatorSet};

// ---------------------------------------------------------------------------
// ERC-7201 namespace slots (computed & pinned against OZ 5.4.0 source)
// Formula: keccak256(abi.encode(uint256(keccak256(id)) - 1)) & ~bytes32(uint256(0xff))
// ---------------------------------------------------------------------------

/// Namespace identifier for OwnableUpgradeable storage.
pub const OWNABLE_NAMESPACE: &str = "openzeppelin.storage.Ownable";

/// Pre-computed ERC-7201 slot for OwnableUpgradeable (OZ 5.4.0).
pub const OWNABLE_SLOT: B256 =
    B256::new(hex!("9016d09d72d40fdae2fd8ceac6b6234c7706214fd39c1cd1e609a0528c199300"));

/// Namespace identifier for ReentrancyGuardUpgradeable storage.
pub const REENTRANCY_GUARD_NAMESPACE: &str = "openzeppelin.storage.ReentrancyGuard";

/// Pre-computed ERC-7201 slot for ReentrancyGuardUpgradeable (OZ 5.4.0).
pub const REENTRANCY_GUARD_SLOT: B256 =
    B256::new(hex!("9b779b17422d0df92223018b32b4d1fa46e071723d6817e2486d003becc55f00"));

/// Namespace identifier for Initializable storage.
pub const INITIALIZABLE_NAMESPACE: &str = "openzeppelin.storage.Initializable";

/// Pre-computed ERC-7201 slot for Initializable (OZ 5.4.0).
pub const INITIALIZABLE_SLOT: B256 =
    B256::new(hex!("f0c57e16840df040f15088dc2f81fe391c3923bec73e23a9662efc9c229c6a00"));

// ---------------------------------------------------------------------------
// EIP-1967 proxy slots
// ---------------------------------------------------------------------------

/// EIP-1967 implementation slot: `bytes32(uint256(keccak256("eip1967.proxy.implementation")) - 1)`
pub const EIP1967_IMPL_SLOT: B256 =
    B256::new(hex!("360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc"));

// ---------------------------------------------------------------------------
// ERC-7201 computation
// ---------------------------------------------------------------------------

/// Compute the ERC-7201 storage slot for a given namespace id string.
///
/// `keccak256(abi.encode(uint256(keccak256(id)) - 1)) & ~bytes32(uint256(0xff))`
pub fn erc7201_slot(namespace: &str) -> B256 {
    let inner_hash = keccak256(namespace.as_bytes());
    let inner_u256 = U256::from_be_bytes(inner_hash.0) - U256::from(1u64);
    let outer_hash = keccak256(inner_u256.to_be_bytes::<32>());
    let outer_u256 = U256::from_be_bytes(outer_hash.0);
    // Mask: clear the lowest byte (& ~0xff)
    let masked = outer_u256 & !(U256::from(0xffu64));
    B256::from(masked.to_be_bytes::<32>())
}

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
    let validator_addresses: Vec<Address> = validator_set
        .ordered_validator_keys()
        .iter()
        .map(validator_address_from_key)
        .collect();

    // Slot stores the length of the dynamic array `_inner._values`
    let length_slot = StorageSlotCalculator::struct_field_slot(base_slot_b256, 0);
    storage.insert(
        length_slot,
        B256::from(U256::from(validator_addresses.len() as u64).to_be_bytes::<32>()),
    );

    // `_inner._positions` mapping is located at slot + 1
    let positions_base_slot_b256 = StorageSlotCalculator::struct_field_slot(base_slot_b256, 1);
    let positions_base_slot = U256::from_be_slice(positions_base_slot_b256.as_slice());

    for (index, address) in validator_addresses.iter().enumerate() {
        // Write array element at base + index
        let element_slot =
            StorageSlotCalculator::array_element_slot(base_slot, U256::from(index as u64));
        storage.insert(element_slot, address.into_word());

        // Write mapping entry with 1-based index
        let position_slot =
            StorageSlotCalculator::mapping_slot(address.into_word(), positions_base_slot);
        storage.insert(
            position_slot,
            B256::from(U256::from((index as u64) + 1).to_be_bytes::<32>()),
        );
    }

    Ok(())
}

/// Set up the validators mapping
pub(crate) fn set_validator_entries_mapping(
    storage: &mut BTreeMap<B256, B256>,
    validator_set: &ValidatorSet,
    base_slot: U256,
) -> Result<()> {
    for validator in validator_set.get_validators() {
        let address = validator_address_from_key(&validator.validator_key);
        let address_word = address.into_word();
        let validator_slot = StorageSlotCalculator::mapping_slot(address_word, base_slot);

        let mut slot_index = U256::from_be_slice(validator_slot.as_slice());
        let (x_limb, y_limb) = validator.validator_key;

        // Store first limb
        storage.insert(validator_slot, B256::from(x_limb.to_be_bytes::<32>()));

        // Store second limb
        slot_index += U256::from(1u64);
        let second_slot = B256::from(slot_index.to_be_bytes::<32>());
        storage.insert(second_slot, B256::from(y_limb.to_be_bytes::<32>()));

        // Store power as uint64 in third slot
        slot_index += U256::from(1u64);
        let power_slot = B256::from(slot_index.to_be_bytes::<32>());
        storage.insert(
            power_slot,
            B256::from(U256::from(validator.power).to_be_bytes::<32>()),
        );
    }

    Ok(())
}

fn validator_address_from_key(key: &ValidatorKey) -> Address {
    let mut raw = [0u8; 64];
    raw[..32].copy_from_slice(&key.0.to_be_bytes::<32>());
    raw[32..].copy_from_slice(&key.1.to_be_bytes::<32>());
    let hash = keccak256(raw);
    Address::from_slice(&hash[12..])
}
