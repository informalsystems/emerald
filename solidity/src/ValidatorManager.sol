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
    using EnumerableSet for EnumerableSet.AddressSet;

    uint256 internal constant SECP256K1_P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F;
    uint256 internal constant SECP256K1_B = 7;
    uint256 internal constant SECP256K1_SQRT_EXPONENT = (SECP256K1_P + 1) / 4;

    struct Secp256k1Key {
        uint256 x;
        uint256 y;
    }

    struct ValidatorInfo {
        Secp256k1Key validatorKey; // Uncompressed secp256k1 key stored as X and Y limbs
        uint64 power; // Voting power
    }

    // State variables
    EnumerableSet.AddressSet private _validatorAddresses;
    mapping(address => ValidatorInfo) private _validators;

    constructor() Ownable(_msgSender()) {}

    // Events
    event ValidatorRegistered(address indexed validatorAddress, Secp256k1Key validatorKey, uint64 power);
    event ValidatorUnregistered(address indexed validatorAddress, Secp256k1Key validatorKey);
    event ValidatorPowerUpdated(
        address indexed validatorAddress, Secp256k1Key validatorKey, uint64 oldPower, uint64 newPower
    );

    // Errors
    error ValidatorAlreadyExists();
    error ValidatorDoesNotExist();
    error InvalidPower();
    error InvalidKey();
    error TotalPowerOverflow();
    error InvalidPublicKeyLength();
    error InvalidPublicKeyFormat();
    error InvalidPublicKeyCoordinates();
    error ModExpFailed();

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
     * @dev Returns the validator address ensuring it already exists.
     */
    function _validatedExistingAddress(Secp256k1Key memory validatorKey)
        internal
        view
        returns (address validatorAddress)
    {
        validatorAddress = _validatorAddress(validatorKey);
        if (!_validatorAddresses.contains(validatorAddress)) {
            revert ValidatorDoesNotExist();
        }
    }

    /**
     * @dev Returns the validator address ensuring it does not exist yet.
     */
    function _validatedNewAddress(Secp256k1Key memory validatorKey) internal view returns (address validatorAddress) {
        validatorAddress = _validatorAddress(validatorKey);
        if (_validatorAddresses.contains(validatorAddress)) {
            revert ValidatorAlreadyExists();
        }
    }

    /**
     * @dev Ensures the derived validator address is already registered.
     */
    function _requireValidatorAddressExists(address validatorAddress) internal view {
        if (!_validatorAddresses.contains(validatorAddress)) {
            revert ValidatorDoesNotExist();
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
        uint256 length = addValidators.length;
        for (uint256 i = 0; i < length;) {
            _register(addValidators[i]);
            unchecked {
                ++i;
            }
        }
    }

    /**
     * @dev Register a new validator from a hex-encoded public key and power.
     *      Accepts either a 33-byte compressed or 65-byte uncompressed secp256k1 key.
     * @param validatorPublicKey The validator public key bytes
     * @param power The voting power for the validator
     */
    function register(bytes calldata validatorPublicKey, uint64 power) external nonReentrant onlyOwner {
        Secp256k1Key memory validatorKey = _secp256k1KeyFromBytes(validatorPublicKey);
        _register(ValidatorInfo({validatorKey: validatorKey, power: power}));
    }

    /**
     * @dev Internal implementation to register a new validator with specified key and power
     * @param validator Validator data containing key and power
     */
    function _register(ValidatorInfo memory validator)
        internal
        validKey(validator.validatorKey)
        validPower(validator.power)
    {
        address validatorAddress = _validatedNewAddress(validator.validatorKey);
        _validators[validatorAddress] = validator;
        _validatorAddresses.add(validatorAddress);

        emit ValidatorRegistered(validatorAddress, validator.validatorKey, validator.power);
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
        uint256 length = validatorKeys.length;
        for (uint256 i = 0; i < length;) {
            _unregister(validatorKeys[i]);
            unchecked {
                ++i;
            }
        }
    }

    /**
     * @dev Unregister a validator (only callable by the owner)
     */
    function unregister(address validatorAddress) external nonReentrant onlyOwner {
        _unregisterByAddress(validatorAddress);
    }

    /**
     * @dev Internal implementation to unregister a validator (only callable by the owner)
     */
    function _unregister(Secp256k1Key memory validatorKey) internal {
        address validatorAddress = _validatedExistingAddress(validatorKey);

        delete _validators[validatorAddress];
        _validatorAddresses.remove(validatorAddress);

        emit ValidatorUnregistered(validatorAddress, validatorKey);
    }

    /**
     * @dev Internal implementation to unregister by validator address.
     */
    function _unregisterByAddress(address validatorAddress) internal {
        _requireValidatorAddressExists(validatorAddress);
        ValidatorInfo memory validator = _validators[validatorAddress];

        delete _validators[validatorAddress];
        _validatorAddresses.remove(validatorAddress);

        emit ValidatorUnregistered(validatorAddress, validator.validatorKey);
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
        validPower(newPower)
    {
        address validatorAddress = _validatedExistingAddress(validatorKey);
        uint64 oldPower = _validators[validatorAddress].power;

        _validators[validatorAddress].power = newPower;

        emit ValidatorPowerUpdated(validatorAddress, validatorKey, oldPower, newPower);
    }

    /**
     * @dev Get validator information by key
     * @param validatorKey The validator key
     * @return info Complete validator info including key and power
     * @dev Reverts with {ValidatorDoesNotExist} if the key is not registered
     */
    function getValidator(Secp256k1Key memory validatorKey) external view returns (ValidatorInfo memory info) {
        address validatorAddress = _validatedExistingAddress(validatorKey);
        return _validators[validatorAddress];
    }

    /**
     * @dev Get all validators with their information
     * @return validators Array of validator information
     */
    function getValidators() external view returns (ValidatorInfo[] memory validators) {
        uint256 length = _validatorAddresses.length();
        validators = new ValidatorInfo[](length);

        for (uint256 i = 0; i < length;) {
            address validatorAddress = _validatorAddresses.at(i);
            validators[i] = _validators[validatorAddress];
            unchecked {
                ++i;
            }
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
     * @dev Check if a key is a registered validator
     * @param validatorKey The validator key to check
     * @return True if the key is a registered validator
     */
    function isValidator(Secp256k1Key memory validatorKey) external view returns (bool) {
        return _validatorAddresses.contains(_validatorAddress(validatorKey));
    }

    /**
     * @dev Get all validator keys
     * @return validatorKeys Array of all validator keys
     */
    function getValidatorKeys() external view returns (Secp256k1Key[] memory validatorKeys) {
        uint256 length = _validatorAddresses.length();
        validatorKeys = new Secp256k1Key[](length);

        for (uint256 i = 0; i < length;) {
            address validatorAddress = _validatorAddresses.at(i);
            validatorKeys[i] = _validators[validatorAddress].validatorKey;
            unchecked {
                ++i;
            }
        }
    }

    /**
     * @dev Get total power of all validators
     * @return The sum of all validator powers
     */
    function getTotalPower() external view returns (uint64) {
        uint256 length = _validatorAddresses.length();
        uint64 total = 0;
        for (uint256 i = 0; i < length;) {
            address validatorAddress = _validatorAddresses.at(i);
            uint64 power = _validators[validatorAddress].power;
            if (total > type(uint64).max - power) {
                revert TotalPowerOverflow();
            }
            total += power;
            unchecked {
                ++i;
            }
        }
        return total;
    }

    /**
     * @dev Computes the deterministic identifier for a validator key.
     */
    function _validatorAddress(Secp256k1Key memory validatorKey) internal pure returns (address) {
        bytes32 hash;
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, mload(validatorKey))
            mstore(add(ptr, 0x20), mload(add(validatorKey, 0x20)))
            hash := keccak256(ptr, 0x40)
        }
        return address(uint160(uint256(hash)));
    }

    function _secp256k1KeyFromBytes(bytes calldata validatorPublicKey) internal view returns (Secp256k1Key memory) {
        if (validatorPublicKey.length == 33) {
            uint8 prefix = uint8(validatorPublicKey[0]);
            if (prefix != 0x02 && prefix != 0x03) {
                revert InvalidPublicKeyFormat();
            }

            uint256 x = _bytesToUintCalldata(validatorPublicKey, 1);
            uint256 y = _deriveYFromX(x, prefix == 0x03);
            if (x == 0 && y == 0) {
                revert InvalidKey();
            }
            return Secp256k1Key({x: x, y: y});
        } else if (validatorPublicKey.length == 65) {
            if (uint8(validatorPublicKey[0]) != 0x04) {
                revert InvalidPublicKeyFormat();
            }

            uint256 x = _bytesToUintCalldata(validatorPublicKey, 1);
            uint256 y = _bytesToUintCalldata(validatorPublicKey, 33);
            if (x == 0 && y == 0) {
                revert InvalidKey();
            }
            _validatePointOnCurve(x, y);
            return Secp256k1Key({x: x, y: y});
        }

        revert InvalidPublicKeyLength();
    }

    function _bytesToUintCalldata(bytes calldata data, uint256 start) internal pure returns (uint256 result) {
        if (data.length < start + 32) {
            revert InvalidPublicKeyLength();
        }
        assembly {
            result := calldataload(add(data.offset, start))
        }
    }

    function _deriveYFromX(uint256 x, bool odd) internal view returns (uint256) {
        if (x >= SECP256K1_P) {
            revert InvalidPublicKeyCoordinates();
        }

        uint256 xx = mulmod(x, x, SECP256K1_P);
        uint256 xxx = mulmod(xx, x, SECP256K1_P);
        uint256 rhs = addmod(xxx, SECP256K1_B, SECP256K1_P);
        uint256 y = _modExp(rhs, SECP256K1_SQRT_EXPONENT);

        if (mulmod(y, y, SECP256K1_P) != rhs) {
            revert InvalidPublicKeyCoordinates();
        }

        if ((y & 1) != (odd ? 1 : 0)) {
            y = SECP256K1_P - y;
        }

        return y;
    }

    function _modExp(uint256 base, uint256 exponent) internal view returns (uint256 result) {
        uint256 modulus = SECP256K1_P;
        bool success;
        assembly {
            let pointer := mload(0x40)
            mstore(pointer, 0x20)
            mstore(add(pointer, 0x20), 0x20)
            mstore(add(pointer, 0x40), 0x20)
            mstore(add(pointer, 0x60), base)
            mstore(add(pointer, 0x80), exponent)
            mstore(add(pointer, 0xa0), modulus)
            success := staticcall(gas(), 0x05, pointer, 0xc0, pointer, 0x20)
            result := mload(pointer)
        }

        if (!success) {
            revert ModExpFailed();
        }
    }

    function _validatePointOnCurve(uint256 x, uint256 y) internal pure {
        if (x >= SECP256K1_P || y >= SECP256K1_P) {
            revert InvalidPublicKeyCoordinates();
        }

        uint256 lhs = mulmod(y, y, SECP256K1_P);
        uint256 xx = mulmod(x, x, SECP256K1_P);
        uint256 rhs = addmod(mulmod(xx, x, SECP256K1_P), SECP256K1_B, SECP256K1_P);

        if (lhs != rhs) {
            revert InvalidPublicKeyCoordinates();
        }
    }
}
