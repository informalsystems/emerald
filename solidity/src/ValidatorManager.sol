// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {EnumerableSet} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/**
 * @title ValidatorManager
 * @dev Manages a set of validators with associated voting power
 * @dev Ownership controls who can register, unregister, and update validator power
 */
contract ValidatorManager is Ownable, ReentrancyGuard {
    using EnumerableSet for EnumerableSet.Bytes32Set;

    struct Secp256k1Key {
        uint256 x;
        uint256 y;
    }

    struct ValidatorInfo {
        Secp256k1Key validatorKey; // Uncompressed secp256k1 key stored as X and Y limbs
        uint64 power; // Voting power
    }

    // State variables
    EnumerableSet.Bytes32Set private _validatorKeys;
    mapping(bytes32 => ValidatorInfo) private _validators;

    constructor() Ownable(_msgSender()) {}

    // Events
    event ValidatorRegistered(bytes32 indexed validatorKeyId, Secp256k1Key validatorKey, uint64 power);
    event ValidatorUnregistered(bytes32 indexed validatorKeyId, Secp256k1Key validatorKey);
    event ValidatorPowerUpdated(
        bytes32 indexed validatorKeyId, Secp256k1Key validatorKey, uint64 oldPower, uint64 newPower
    );

    // Errors
    error ValidatorAlreadyExists();
    error ValidatorDoesNotExist();
    error InvalidPower();
    error InvalidKey();
    error TotalPowerOverflow();

    /**
     * @dev Modifier to check if a validator exists
     * @param validatorKey The validator key to check
     */
    modifier validatorExists(Secp256k1Key memory validatorKey) {
        _requireValidatorExists(validatorKey);
        _;
    }

    /**
     * @dev Modifier to check if a validator does not exist
     * @param validatorKey The validator key to check
     */
    modifier validatorNotExists(Secp256k1Key memory validatorKey) {
        _requireValidatorNotExists(validatorKey);
        _;
    }

    /**
     * @dev Modifier to check if power is valid (greater than 0)
     * @param power The power value to validate
     */
    modifier validPower(uint64 power) {
        _requireValidPower(power);
        _;
    }

    /**
     * @dev Modifier to check if validator key is valid (not zero)
     * @param validatorKey The validator key to validate
     */
    modifier validKey(Secp256k1Key memory validatorKey) {
        _requireValidKey(validatorKey);
        _;
    }

    /**
     * @dev Ensures the provided validator key is already registered.
     */
    function _requireValidatorExists(Secp256k1Key memory validatorKey) internal view {
        if (!_validatorKeys.contains(_validatorKeyId(validatorKey))) {
            revert ValidatorDoesNotExist();
        }
    }

    /**
     * @dev Ensures the provided validator key has not been registered yet.
     */
    function _requireValidatorNotExists(Secp256k1Key memory validatorKey) internal view {
        if (_validatorKeys.contains(_validatorKeyId(validatorKey))) {
            revert ValidatorAlreadyExists();
        }
    }

    /**
     * @dev Validates that the provided voting power is non-zero.
     */
    function _requireValidPower(uint64 power) internal pure {
        if (power == 0) {
            revert InvalidPower();
        }
    }

    /**
     * @dev Validates that the validator key is not the all-zero point.
     */
    function _requireValidKey(Secp256k1Key memory validatorKey) internal pure {
        if (validatorKey.x == 0 && validatorKey.y == 0) {
            revert InvalidKey();
        }
    }

    /**
     * @dev Batch register validators.
     * @param addValidators Array of validator key identifiers and power to be added
     * @param removeValidatorKeys Array of validator key identifiers to be removed
     */
    function addAndRemove(ValidatorInfo[] memory addValidators, Secp256k1Key[] memory removeValidatorKeys)
        external
        nonReentrant
        onlyOwner
    {
        _registerSet(addValidators);
        _unregisterSet(removeValidatorKeys);
    }

    /**
     * @dev Batch register validators.
     * @param addValidators Array of validator key identifiers and power to be added
     */
    function registerSet(ValidatorInfo[] memory addValidators) external nonReentrant onlyOwner {
        _registerSet(addValidators);
    }

    /**
     * @dev Internal implementation of batch register validators
     * @param addValidators Array of validator key identifiers and power to be added
     */
    function _registerSet(ValidatorInfo[] memory addValidators) internal {
        for (uint256 i = 0; i < addValidators.length; i++) {
            _register(addValidators[i]);
        }
    }

    /**
     * @dev Register a new validator with specified key and power
     * @param validatorKey The validator key identifier
     * @param power The voting power for the validator
     */
    function register(Secp256k1Key memory validatorKey, uint64 power) external nonReentrant onlyOwner {
        _register(ValidatorInfo({validatorKey: validatorKey, power: power}));
    }

    /**
     * @dev Internal implementation to register a new validator with specified key and power
     * @param validator Validator data containing key and power
     */
    function _register(ValidatorInfo memory validator)
        internal
        validatorNotExists(validator.validatorKey)
        validKey(validator.validatorKey)
        validPower(validator.power)
    {
        bytes32 keyId = _validatorKeyId(validator.validatorKey);
        _validators[keyId] = validator;
        _validatorKeys.add(keyId);

        emit ValidatorRegistered(keyId, validator.validatorKey, validator.power);
    }

    /**
     * @dev Batch unregister validators.
     * @param validatorKeys Array of validator key identifiers
     */
    function unregisterSet(Secp256k1Key[] memory validatorKeys) external nonReentrant onlyOwner {
        _unregisterSet(validatorKeys);
    }

    /**
     * @dev Internal implementation of batch unregister validators
     * @param validatorKeys Array of validator key identifiers
     */
    function _unregisterSet(Secp256k1Key[] memory validatorKeys) internal {
        for (uint256 i = 0; i < validatorKeys.length; i++) {
            _unregister(validatorKeys[i]);
        }
    }

    /**
     * @dev Unregister a validator (only callable by the owner)
     */
    function unregister(Secp256k1Key memory validatorKey) external nonReentrant onlyOwner {
        _unregister(validatorKey);
    }

    /**
     * @dev Internal implementation to unregister a validator (only callable by the owner)
     */
    function _unregister(Secp256k1Key memory validatorKey) internal validatorExists(validatorKey) {
        bytes32 keyId = _validatorKeyId(validatorKey);

        delete _validators[keyId];
        _validatorKeys.remove(keyId);

        emit ValidatorUnregistered(keyId, validatorKey);
    }

    /**
     * @dev Update a validator's power (only callable by the owner)
     * @param validatorKey The validator key to update
     * @param newPower The new voting power
     */
    function updatePower(Secp256k1Key memory validatorKey, uint64 newPower)
        external
        nonReentrant
        onlyOwner
        validatorExists(validatorKey)
        validPower(newPower)
    {
        bytes32 keyId = _validatorKeyId(validatorKey);
        uint64 oldPower = _validators[keyId].power;

        _validators[keyId].power = newPower;

        emit ValidatorPowerUpdated(keyId, validatorKey, oldPower, newPower);
    }

    /**
     * @dev Get validator information by key
     * @param validatorKey The validator key
     * @return info Complete validator info including key and power
     * @dev Reverts with {ValidatorDoesNotExist} if the key is not registered
     */
    function getValidator(Secp256k1Key memory validatorKey)
        external
        view
        validatorExists(validatorKey)
        returns (ValidatorInfo memory info)
    {
        return _validators[_validatorKeyId(validatorKey)];
    }

    /**
     * @dev Get all validators with their information
     * @return validators Array of validator information
     */
    function getValidators() external view returns (ValidatorInfo[] memory validators) {
        uint256 length = _validatorKeys.length();
        validators = new ValidatorInfo[](length);

        for (uint256 i = 0; i < length; i++) {
            bytes32 keyId = _validatorKeys.at(i);
            validators[i] = _validators[keyId];
        }
    }

    /**
     * @dev Get the total number of validators
     * @return The number of registered validators
     */
    function getValidatorCount() external view returns (uint256) {
        return _validatorKeys.length();
    }

    /**
     * @dev Check if a key is a registered validator
     * @param validatorKey The validator key to check
     * @return True if the key is a registered validator
     */
    function isValidator(Secp256k1Key memory validatorKey) external view returns (bool) {
        return _validatorKeys.contains(_validatorKeyId(validatorKey));
    }

    /**
     * @dev Get all validator keys
     * @return validatorKeys Array of all validator keys
     */
    function getValidatorKeys() external view returns (Secp256k1Key[] memory validatorKeys) {
        uint256 length = _validatorKeys.length();
        validatorKeys = new Secp256k1Key[](length);

        for (uint256 i = 0; i < length; i++) {
            validatorKeys[i] = _validators[_validatorKeys.at(i)].validatorKey;
        }
    }

    /**
     * @dev Get total power of all validators
     * @return The sum of all validator powers
     */
    function getTotalPower() external view returns (uint64) {
        uint256 length = _validatorKeys.length();
        uint64 total = 0;
        for (uint256 i = 0; i < length; i++) {
            bytes32 keyId = _validatorKeys.at(i);
            uint64 power = _validators[keyId].power;
            if (total > type(uint64).max - power) {
                revert TotalPowerOverflow();
            }
            total += power;
        }
        return total;
    }

    /**
     * @dev Computes the deterministic identifier for a validator key.
     */
    function _validatorKeyId(Secp256k1Key memory validatorKey) internal pure returns (bytes32 keyId) {
        assembly {
            keyId := keccak256(validatorKey, 0x40)
        }
    }
}
