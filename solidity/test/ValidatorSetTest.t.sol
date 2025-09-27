// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test, console} from "forge-std/Test.sol";
import {ValidatorSet} from "../src/ValidatorSet.sol";

contract ValidatorSetTest is Test {
    ValidatorSet public validatorSet;

    // -- Test Users --
    address alice = makeAddr("alice");
    address bob = makeAddr("bob");

    // -- Test Data --
    bytes32 constant ALICE_PK = keccak256("alice_pubkey");
    bytes32 constant BOB_PK = keccak256("bob_pubkey");
    uint64 constant INITIAL_VOTING_POWER = 100;
    uint64 constant UPDATED_VOTING_POWER = 200;

    function setUp() public {
        validatorSet = new ValidatorSet();
    }

    // -- Registration Tests --

    function test_Register_Success() public {
        vm.prank(alice);

        // Expect the event to be emitted with correct parameters
        vm.expectEmit(true, true, true, true);
        emit ValidatorSet.ValidatorRegistered(alice, ALICE_PK, INITIAL_VOTING_POWER);

        // Register Alice
        validatorSet.register(ALICE_PK, INITIAL_VOTING_POWER);

        // Verify state
        (bytes32 pk, uint64 power) = validatorSet.validators(alice);
        assertEq(pk, ALICE_PK);
        assertEq(power, INITIAL_VOTING_POWER);
    }

    function test_Fail_Register_WhenAlreadyRegistered() public {
        // Alice registers successfully first
        vm.prank(alice);
        validatorSet.register(ALICE_PK, INITIAL_VOTING_POWER);

        // Alice attempts to register again
        vm.prank(alice);
        vm.expectRevert(abi.encodeWithSelector(ValidatorSet.ValidatorAlreadyRegistered.selector, alice));
        validatorSet.register(ALICE_PK, INITIAL_VOTING_POWER);
    }

    // -- Unregistration Tests --

    function test_Unregister_Success() public {
        // Prerequisite: Alice must be registered
        vm.prank(alice);
        validatorSet.register(ALICE_PK, INITIAL_VOTING_POWER);

        // Unregister Alice
        vm.prank(alice);
        vm.expectEmit(true, false, false, true);
        emit ValidatorSet.ValidatorUnregistered(alice);
        validatorSet.unregister();

        // Verify state is deleted
        (bytes32 pk, uint64 power) = validatorSet.validators(alice);
        assertEq(pk, bytes32(0));
        assertEq(power, 0);
    }

    function test_Fail_Unregister_WhenNotRegistered() public {
        // Bob (who is not registered) attempts to unregister
        vm.prank(bob);
        vm.expectRevert(abi.encodeWithSelector(ValidatorSet.ValidatorNotRegistered.selector, bob));
        validatorSet.unregister();
    }

    // -- Voting Power Update Tests --

    function test_UpdateVotingPower_Success() public {
        // Prerequisite: Alice must be registered
        vm.prank(alice);
        validatorSet.register(ALICE_PK, INITIAL_VOTING_POWER);

        // Update voting power for Alice
        vm.prank(alice);
        vm.expectEmit(true, false, false, true);
        emit ValidatorSet.VotingPowerUpdated(alice, UPDATED_VOTING_POWER);
        validatorSet.updateVotingPower(UPDATED_VOTING_POWER);

        // Verify state
        (bytes32 pk, uint64 power) = validatorSet.validators(alice);
        assertEq(power, UPDATED_VOTING_POWER);
        // Ensure the public key is unchanged
        assertEq(pk, ALICE_PK);
    }

    function test_Fail_UpdateVotingPower_WhenNotRegistered() public {
        // Bob (who is not registered) attempts to update voting power
        vm.prank(bob);
        vm.expectRevert(abi.encodeWithSelector(ValidatorSet.ValidatorNotRegistered.selector, bob));
        validatorSet.updateVotingPower(UPDATED_VOTING_POWER);
    }

    // -- Query and Sorting Tests --

    function test_SortedInsertionAndRemoval() public {
        // Create three users with non-sequential addresses to test sorting
        address user1 = address(0x1000);
        address user2 = address(0x3000);
        address user3 = address(0x2000);

        // Register in a non-sorted order
        vm.prank(user2);
        validatorSet.register(keccak256("pk2"), INITIAL_VOTING_POWER);
        vm.prank(user1);
        validatorSet.register(keccak256("pk1"), INITIAL_VOTING_POWER);
        vm.prank(user3);
        validatorSet.register(keccak256("pk3"), INITIAL_VOTING_POWER);

        // The list should now be sorted by address: user1, user3, user2
        address[] memory addresses = validatorSet.getValidatorAddresses();
        assertEq(addresses.length, 3);
        assertEq(addresses[0], user1, "Initial sort [0]: should be user1");
        assertEq(addresses[1], user3, "Initial sort [1]: should be user3");
        assertEq(addresses[2], user2, "Initial sort [2]: should be user2");

        // Unregister the middle element (user3)
        vm.prank(user3);
        validatorSet.unregister();

        // The list should remain sorted: user1, user2
        addresses = validatorSet.getValidatorAddresses();
        assertEq(addresses.length, 2);
        assertEq(addresses[0], user1, "After removing middle [0]: should be user1");
        assertEq(addresses[1], user2, "After removing middle [1]: should be user2");

        // Unregister the first element (user1)
        vm.prank(user1);
        validatorSet.unregister();

        // The list should remain sorted: user2
        addresses = validatorSet.getValidatorAddresses();
        assertEq(addresses.length, 1);
        assertEq(addresses[0], user2, "After removing first [0]: should be user2");

        // Unregister the last element (user2)
        vm.prank(user2);
        validatorSet.unregister();

        // The list should be empty
        addresses = validatorSet.getValidatorAddresses();
        assertEq(addresses.length, 0, "Final list should be empty");
    }

    function test_GetValidators_IsSorted() public {
        // Create two users with non-sequential addresses
        address user1 = address(0x2000);
        address user2 = address(0x1000);
        bytes32 PK1 = keccak256("pk1");
        bytes32 PK2 = keccak256("pk2");

        // Register in non-sorted order
        vm.prank(user1);
        validatorSet.register(PK1, INITIAL_VOTING_POWER);
        vm.prank(user2);
        validatorSet.register(PK2, UPDATED_VOTING_POWER);

        ValidatorSet.ValidatorDetails[] memory validators = validatorSet.getValidators();
        assertEq(validators.length, 2);

        // Check user2's data (should be at index 0 because address is smaller)
        assertEq(validators[0].ethAddress, user2);
        assertEq(validators[0].ed25519PublicKey, PK2);
        assertEq(validators[0].votingPower, UPDATED_VOTING_POWER);

        // Check user1's data (should be at index 1)
        assertEq(validators[1].ethAddress, user1);
        assertEq(validators[1].ed25519PublicKey, PK1);
        assertEq(validators[1].votingPower, INITIAL_VOTING_POWER);
    }

    function test_GetValidator_Success() public {
        // Prerequisite: Alice must be registered
        vm.prank(alice);
        validatorSet.register(ALICE_PK, INITIAL_VOTING_POWER);

        // Get Alice's validator details
        ValidatorSet.ValidatorDetails memory aliceValidator = validatorSet.getValidator(alice);

        // Assert the details are correct
        assertEq(aliceValidator.ethAddress, alice);
        assertEq(aliceValidator.ed25519PublicKey, ALICE_PK);
        assertEq(aliceValidator.votingPower, INITIAL_VOTING_POWER);
    }

    function test_Fail_GetValidator_WhenNotRegistered() public {
        // Bob is not registered, so this should fail
        vm.expectRevert(abi.encodeWithSelector(ValidatorSet.ValidatorNotRegistered.selector, bob));
        validatorSet.getValidator(bob);
    }
}
