// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/**
 * @title ValidatorManager
 * @dev Manages a set of validators with associated voting power
 * @dev Ownership controls who can register, unregister, and update validator power
 */
contract ValidatorManager is Ownable, ReentrancyGuard {
    using EnumerableSet for EnumerableSet.UintSet;

    struct ValidatorInfo {
        uint256 validatorKey; // Validator identifier
        uint256 power; // Voting power
    }

    // State variables
    EnumerableSet.UintSet private _validatorKeys;
    mapping(uint256 => uint256) private _validatorPowers;

    constructor() Ownable(_msgSender()) {}

    // Events
    event ValidatorRegistered(uint256 indexed validatorKey, uint256 power);
    event ValidatorUnregistered(uint256 indexed validatorKey);
    event ValidatorPowerUpdated(uint256 indexed validatorKey, uint256 oldPower, uint256 newPower);

    // Errors
    error ValidatorAlreadyExists();
    error ValidatorDoesNotExist();
    error InvalidPower();
    error InvalidKey();

    /**
     * @dev Modifier to check if a validator exists
     * @param validatorKey The validator key to check
     */
    modifier validatorExists(uint256 validatorKey) {
        if (!_validatorKeys.contains(validatorKey)) {
            revert ValidatorDoesNotExist();
        }
        _;
    }

    /**
     * @dev Modifier to check if a validator does not exist
     * @param validatorKey The validator key to check
     */
    modifier validatorNotExists(uint256 validatorKey) {
        if (_validatorKeys.contains(validatorKey)) {
            revert ValidatorAlreadyExists();
        }
        _;
    }

    /**
     * @dev Modifier to check if power is valid (greater than 0)
     * @param power The power value to validate
     */
    modifier validPower(uint256 power) {
        if (power == 0) {
            revert InvalidPower();
        }
        _;
    }

    /**
     * @dev Modifier to check if validator key is valid (not zero)
     * @param validatorKey The validator key to validate
     */
    modifier validKey(uint256 validatorKey) {
        if (validatorKey == 0) {
            revert InvalidKey();
        }
        _;
    }

    /**
     * @dev Register a new validator with specified key and power
     * @param validatorKey The validator key identifier
     * @param power The voting power for the validator
     */
    function register(uint256 validatorKey, uint256 power)
        external
        nonReentrant
        onlyOwner
        validatorNotExists(validatorKey)
        validKey(validatorKey)
        validPower(power)
    {
        _validatorPowers[validatorKey] = power;
        _validatorKeys.add(validatorKey);

        emit ValidatorRegistered(validatorKey, power);
    }

    /**
     * @dev Unregister a validator (only callable by the owner)
     */
    function unregister(uint256 validatorKey) external nonReentrant onlyOwner validatorExists(validatorKey) {
        delete _validatorPowers[validatorKey];
        _validatorKeys.remove(validatorKey);

        emit ValidatorUnregistered(validatorKey);
    }

    /**
     * @dev Update a validator's power (only callable by the owner)
     * @param validatorKey The validator key to update
     * @param newPower The new voting power
     */
    function updatePower(uint256 validatorKey, uint256 newPower)
        external
        nonReentrant
        onlyOwner
        validatorExists(validatorKey)
        validPower(newPower)
    {
        uint256 oldPower = _validatorPowers[validatorKey];

        _validatorPowers[validatorKey] = newPower;

        emit ValidatorPowerUpdated(validatorKey, oldPower, newPower);
    }

    /**
     * @dev Get validator information by key
     * @param validatorKey The validator key
     * @return info Complete validator info including key and power
     * @dev Reverts with {ValidatorDoesNotExist} if the key is not registered
     */
    function getValidator(uint256 validatorKey)
        external
        view
        validatorExists(validatorKey)
        returns (ValidatorInfo memory info)
    {
        return ValidatorInfo({validatorKey: validatorKey, power: _validatorPowers[validatorKey]});
    }

    /**
     * @dev Get all validators with their information
     * @return validators Array of validator information
     */
    function getValidators() external view returns (ValidatorInfo[] memory validators) {
        uint256 length = _validatorKeys.length();
        validators = new ValidatorInfo[](length);

        for (uint256 i = 0; i < length; i++) {
            uint256 validatorKey = _validatorKeys.at(i);
            validators[i] = ValidatorInfo({validatorKey: validatorKey, power: _validatorPowers[validatorKey]});
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
    function isValidator(uint256 validatorKey) external view returns (bool) {
        return _validatorKeys.contains(validatorKey);
    }

    /**
     * @dev Get all validator keys
     * @return validatorKeys Array of all validator keys
     */
    function getValidatorKeys() external view returns (uint256[] memory validatorKeys) {
        return _validatorKeys.values();
    }

    /**
     * @dev Get total power of all validators
     * @return The sum of all validator powers
     */
    function getTotalPower() external view returns (uint256) {
        uint256 total = 0;
        uint256 length = _validatorKeys.length();

        for (uint256 i = 0; i < length; i++) {
            uint256 validatorKey = _validatorKeys.at(i);
            total += _validatorPowers[validatorKey];
        }

        return total;
    }
}
