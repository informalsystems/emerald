// SPDX-License-Identifier: MIT
pragma solidity ^0.8.27;

import "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import "@openzeppelin/contracts/utils/Context.sol";

/**
 * @title ValidatorSet
 * @dev A contract for managing a set of validators with their voting power
 * @dev Validators can register, unregister, and update their own power
 */
contract ValidatorSet is Context, ReentrancyGuard {
    using EnumerableSet for EnumerableSet.AddressSet;

    struct ValidatorInfo {
        bytes32 ed25519Key; // Ed25519 public key
        uint256 power; // Voting power
    }

    struct ValidatorInfoFull {
        address validator; // Validator address
        bytes32 ed25519Key; // Ed25519 public key
        uint256 power; // Voting power
    }

    // State variables
    EnumerableSet.AddressSet private _validatorAddresses;
    mapping(address => ValidatorInfo) private _validators;

    // Events
    event ValidatorRegistered(address indexed validator, bytes32 indexed ed25519Key, uint256 power);
    event ValidatorUnregistered(address indexed validator);
    event ValidatorPowerUpdated(address indexed validator, uint256 oldPower, uint256 newPower);
    event ValidatorKeyUpdated(address indexed validator, bytes32 oldKey, bytes32 newKey);

    // Errors
    error ValidatorAlreadyExists();
    error ValidatorDoesNotExist();
    error InvalidPower();
    error InvalidKey();
    error UnauthorizedValidator();

    /**
     * @dev Modifier to check if a validator exists
     * @param validator The address to check
     */
    modifier validatorExists(address validator) {
        if (!_validatorAddresses.contains(validator)) {
            revert ValidatorDoesNotExist();
        }
        _;
    }

    /**
     * @dev Modifier to check if a validator does not exist
     * @param validator The address to check
     */
    modifier validatorNotExists(address validator) {
        if (_validatorAddresses.contains(validator)) {
            revert ValidatorAlreadyExists();
        }
        _;
    }

    /**
     * @dev Modifier to ensure only the validator can modify their own data
     * @param validator The validator address that should match the caller
     */
    modifier onlyValidator(address validator) {
        if (_msgSender() != validator) {
            revert UnauthorizedValidator();
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
     * @dev Modifier to check if Ed25519 key is valid (not zero)
     * @param key The Ed25519 public key to validate
     */
    modifier validKey(bytes32 key) {
        if (key == bytes32(0)) {
            revert InvalidKey();
        }
        _;
    }

    /**
     * @dev Register a new validator with specified Ed25519 key and power
     * @param ed25519Key The Ed25519 public key for the validator
     * @param power The voting power for the validator
     */
    function register(bytes32 ed25519Key, uint256 power)
        external
        nonReentrant
        validatorNotExists(_msgSender())
        validKey(ed25519Key)
        validPower(power)
    {
        address validator = _msgSender();

        _validators[validator] = ValidatorInfo({ed25519Key: ed25519Key, power: power});
        _validatorAddresses.add(validator);

        emit ValidatorRegistered(validator, ed25519Key, power);
    }

    /**
     * @dev Unregister a validator (only the validator can unregister themselves)
     */
    function unregister() external nonReentrant validatorExists(_msgSender()) onlyValidator(_msgSender()) {
        address validator = _msgSender();

        delete _validators[validator];
        _validatorAddresses.remove(validator);

        emit ValidatorUnregistered(validator);
    }

    /**
     * @dev Update validator's power (only the validator can update their own power)
     * @param newPower The new voting power
     */
    function updatePower(uint256 newPower)
        external
        nonReentrant
        validatorExists(_msgSender())
        onlyValidator(_msgSender())
        validPower(newPower)
    {
        address validator = _msgSender();
        uint256 oldPower = _validators[validator].power;

        _validators[validator].power = newPower;

        emit ValidatorPowerUpdated(validator, oldPower, newPower);
    }

    /**
     * @dev Update validator's Ed25519 key (only the validator can update their own key)
     * @param newKey The new Ed25519 public key
     */
    function updateKey(bytes32 newKey)
        external
        nonReentrant
        validatorExists(_msgSender())
        onlyValidator(_msgSender())
        validKey(newKey)
    {
        address validator = _msgSender();
        bytes32 oldKey = _validators[validator].ed25519Key;

        _validators[validator].ed25519Key = newKey;

        emit ValidatorKeyUpdated(validator, oldKey, newKey);
    }

    /**
     * @dev Get validator information by address
     * @param validator The validator address
     * @return info Complete validator info including address, key, and power
     * @dev Reverts with {ValidatorDoesNotExist} if the address is not registered
     */
    function getValidator(address validator) external view returns (ValidatorInfoFull memory info) {
        if (!_validatorAddresses.contains(validator)) {
            revert ValidatorDoesNotExist();
        }

        ValidatorInfo memory stored = _validators[validator];
        return ValidatorInfoFull({validator: validator, ed25519Key: stored.ed25519Key, power: stored.power});
    }

    /**
     * @dev Get all validators with their information
     * @return validators Array of validator information
     */
    function getValidators() external view returns (ValidatorInfoFull[] memory validators) {
        uint256 length = _validatorAddresses.length();
        validators = new ValidatorInfoFull[](length);

        for (uint256 i = 0; i < length; i++) {
            address validatorAddr = _validatorAddresses.at(i);
            ValidatorInfo memory info = _validators[validatorAddr];
            validators[i] =
                ValidatorInfoFull({validator: validatorAddr, ed25519Key: info.ed25519Key, power: info.power});
        }
    }

    /**
     * @dev Get the total number of validators
     * @return The number of registered validators
     */
    function getValidatorCount() external view returns (uint256) {
        return _validatorAddresses.length();
    }

    /**
     * @dev Check if an address is a registered validator
     * @param validator The address to check
     * @return True if the address is a registered validator
     */
    function isValidator(address validator) external view returns (bool) {
        return _validatorAddresses.contains(validator);
    }

    /**
     * @dev Get all validator addresses
     * @return addresses Array of all validator addresses
     */
    function getValidatorAddresses() external view returns (address[] memory addresses) {
        uint256 length = _validatorAddresses.length();
        addresses = new address[](length);

        for (uint256 i = 0; i < length; i++) {
            addresses[i] = _validatorAddresses.at(i);
        }
    }

    /**
     * @dev Get total power of all validators
     * @return The sum of all validator powers
     */
    function getTotalPower() external view returns (uint256) {
        uint256 total = 0;
        uint256 length = _validatorAddresses.length();

        for (uint256 i = 0; i < length; i++) {
            address validatorAddr = _validatorAddresses.at(i);
            total += _validators[validatorAddr].power;
        }

        return total;
    }
}
