// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title ValidatorSet
 * @notice Manages a set of validators for a consensus network.
 * @dev Validators can register, unregister, and update their voting power.
 * This contract maintains a canonically sorted list of validator addresses.
 */
contract ValidatorSet {
    // -- State --

    struct Validator {
        bytes32 ed25519PublicKey; // The consensus public key (e.g., Ed25519).
        uint64 votingPower;      // The validator's voting power.
    }

    struct ValidatorDetails {
        address ethAddress;      // The Ethereum address of the validator.
        bytes32 ed25519PublicKey; // The consensus public key (e.g., Ed25519).
        uint64 votingPower;      // The validator's voting power.
    }

    // Mapping from the validator's Ethereum address to their consensus details.
    mapping(address => Validator) public validators;
    // Array to store all registered validator addresses for easy iteration.
    // This array is always kept sorted by address.
    address[] public validatorAddresses;
    // Mapping to store the index of each validator in the validatorAddresses array.
    mapping(address => uint256) private validatorAddressIndex;


    // -- Events --

    event ValidatorRegistered(address indexed validatorAddress, bytes32 ed25519PublicKey, uint64 votingPower);
    event ValidatorUnregistered(address indexed validatorAddress);
    event VotingPowerUpdated(address indexed validatorAddress, uint64 newVotingPower);

    // -- Errors --

    error ValidatorAlreadyRegistered(address validatorAddress);
    error ValidatorNotRegistered(address validatorAddress);


    // -- Modifiers --

    modifier onlyWhenRegistered(address _validator) {
        if (validators[_validator].ed25519PublicKey == bytes32(0)) {
            revert ValidatorNotRegistered(_validator);
        }
        _;
    }

    modifier onlyWhenNotRegistered(address _validator) {
        if (validators[_validator].ed25519PublicKey != bytes32(0)) {
            revert ValidatorAlreadyRegistered(_validator);
        }
        _;
    }

    // -- Functions --

    /**
     * @notice Registers the calling address as a validator, inserting it into the sorted set.
     * @dev This operation is O(N) due to the sorted insertion.
     * @param _ed25519PublicKey The 32-byte Ed25519 public key.
     * @param _votingPower The initial voting power for the validator.
     */
    function register(bytes32 _ed25519PublicKey, uint64 _votingPower) external onlyWhenNotRegistered(msg.sender) {
        validators[msg.sender] = Validator({
            ed25519PublicKey: _ed25519PublicKey,
            votingPower: _votingPower
        });

        _insertSorted(msg.sender);

        emit ValidatorRegistered(msg.sender, _ed25519PublicKey, _votingPower);
    }

    /**
     * @notice Unregisters the calling address, removing them from the sorted validator set.
     * @dev This operation is O(N) due to maintaining the sorted order.
     */
    function unregister() external onlyWhenRegistered(msg.sender) {
        _removeSorted(msg.sender);
        
        delete validators[msg.sender];
        emit ValidatorUnregistered(msg.sender);
    }

    /**
     * @notice Updates the voting power for the calling address.
     * @param _newVotingPower The new voting power.
     */
    function updateVotingPower(uint64 _newVotingPower) external onlyWhenRegistered(msg.sender) {
        validators[msg.sender].votingPower = _newVotingPower;
        emit VotingPowerUpdated(msg.sender, _newVotingPower);
    }

    // -- View Functions --

    /**
     * @notice Returns the sorted list of all registered validator addresses.
     * @return An array of addresses sorted in ascending order.
     */
    function getValidatorAddresses() external view returns (address[] memory) {
        return validatorAddresses;
    }

    /**
     * @notice Returns the details for all registered validators, sorted by address.
     * @return An array of Validator structs, sorted by ethAddress.
     */
    function getValidators() external view returns (ValidatorDetails[] memory) {
        uint256 validatorCount = validatorAddresses.length;
        ValidatorDetails[] memory _validators = new ValidatorDetails[](validatorCount);

        for (uint i = 0; i < validatorCount; i++) {
            _validators[i] = ValidatorDetails({
                ethAddress: validatorAddresses[i],
                ed25519PublicKey: validators[validatorAddresses[i]].ed25519PublicKey,
                votingPower: validators[validatorAddresses[i]].votingPower
            });
        }
        
        return _validators;
    }

    /**
     * @notice Gets the details for a specific validator.
     * @param _validatorAddress The address of the validator to query.
     * @return The Validator struct for the given address.
     */
    function getValidator(address _validatorAddress) external view onlyWhenRegistered(_validatorAddress) returns (ValidatorDetails memory) {
        return ValidatorDetails({
            ethAddress: _validatorAddress,
            ed25519PublicKey: validators[_validatorAddress].ed25519PublicKey,
            votingPower: validators[_validatorAddress].votingPower
        });
    }

    // -- Internal Functions --

    /**
     * @dev Inserts a new validator address into the `validatorAddresses` array while maintaining sort order.
     * Updates the `validatorAddressIndex` for all affected elements.
     */
    function _insertSorted(address _newValidator) private {
        uint256 len = validatorAddresses.length;
        uint256 insertionIndex = 0;
        // Find the insertion point
        while (insertionIndex < len && validatorAddresses[insertionIndex] < _newValidator) {
            insertionIndex++;
        }

        // Add a new empty slot at the end
        validatorAddresses.push();

        // Shift elements to the right to make space
        for (uint256 i = len; i > insertionIndex; i--) {
            address addrToShift = validatorAddresses[i - 1];
            validatorAddresses[i] = addrToShift;
            validatorAddressIndex[addrToShift] = i;
        }

        // Insert the new validator
        validatorAddresses[insertionIndex] = _newValidator;
        validatorAddressIndex[_newValidator] = insertionIndex;
    }
    
    /**
     * @dev Removes a validator address from the `validatorAddresses` array, shifting elements to maintain a sorted list.
     * Updates the `validatorAddressIndex` for all affected elements.
     */
    function _removeSorted(address _validatorToRemove) private {
        uint256 indexToRemove = validatorAddressIndex[_validatorToRemove];
        uint256 len = validatorAddresses.length;

        // Shift elements to the left to fill the gap
        for (uint256 i = indexToRemove; i < len - 1; i++) {
            address addrToShift = validatorAddresses[i + 1];
            validatorAddresses[i] = addrToShift;
            validatorAddressIndex[addrToShift] = i;
        }
        
        // Remove the last element and delete the index entry
        validatorAddresses.pop();
        delete validatorAddressIndex[_validatorToRemove];
    }
}

