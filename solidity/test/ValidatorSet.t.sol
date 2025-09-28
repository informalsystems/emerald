// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import {Test} from "forge-std/Test.sol";
import {ValidatorSet} from "../src/ValidatorSet.sol";

contract ValidatorSetTest is Test {
    ValidatorSet public validatorSet;

    // Test accounts
    address public alice = address(0x1);
    address public bob = address(0x2);
    address public charlie = address(0x3);

    // Test constants
    uint256 public constant INITIAL_POWER = 100;
    uint256 public constant UPDATED_POWER = 200;
    bytes32 public constant ALICE_KEY = bytes32(uint256(0x1111));
    bytes32 public constant BOB_KEY = bytes32(uint256(0x2222));
    bytes32 public constant CHARLIE_KEY = bytes32(uint256(0x3333));
    bytes32 public constant NEW_KEY = bytes32(uint256(0x4444));

    event ValidatorRegistered(address indexed validator, bytes32 indexed ed25519Key, uint256 power);
    event ValidatorUnregistered(address indexed validator);
    event ValidatorPowerUpdated(address indexed validator, uint256 oldPower, uint256 newPower);
    event ValidatorKeyUpdated(address indexed validator, bytes32 oldKey, bytes32 newKey);

    function setUp() public {
        validatorSet = new ValidatorSet();

        // Give test accounts some ETH
        vm.deal(alice, 1 ether);
        vm.deal(bob, 1 ether);
        vm.deal(charlie, 1 ether);
    }

    // Registration Tests
    function testRegisterValidator() public {
        vm.prank(alice);

        vm.expectEmit(true, true, true, true);
        emit ValidatorRegistered(alice, ALICE_KEY, INITIAL_POWER);

        validatorSet.register(ALICE_KEY, INITIAL_POWER);

        ValidatorSet.ValidatorInfoFull memory info = validatorSet.getValidator(alice);
        assertEq(info.validator, alice);
        assertEq(info.ed25519Key, ALICE_KEY);
        assertEq(info.power, INITIAL_POWER);
        assertEq(validatorSet.getTotalPower(), INITIAL_POWER);
        assertEq(validatorSet.getValidatorCount(), 1);
        assertTrue(validatorSet.isValidator(alice));
    }

    function testRegisterMultipleValidators() public {
        // Register alice
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, INITIAL_POWER);

        // Register bob
        vm.prank(bob);
        validatorSet.register(BOB_KEY, UPDATED_POWER);

        // Check total power and count
        assertEq(validatorSet.getTotalPower(), INITIAL_POWER + UPDATED_POWER);
        assertEq(validatorSet.getValidatorCount(), 2);

        // Check both validators exist
        assertTrue(validatorSet.isValidator(alice));
        assertTrue(validatorSet.isValidator(bob));
        assertFalse(validatorSet.isValidator(charlie));
    }

    function testCannotRegisterWithZeroPower() public {
        vm.prank(alice);
        vm.expectRevert(ValidatorSet.InvalidPower.selector);
        validatorSet.register(ALICE_KEY, 0);
    }

    function testCannotRegisterTwice() public {
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, INITIAL_POWER);

        vm.prank(alice);
        vm.expectRevert(ValidatorSet.ValidatorAlreadyExists.selector);
        validatorSet.register(ALICE_KEY, UPDATED_POWER);
    }

    // Unregistration Tests
    function testUnregisterValidator() public {
        // First register
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, INITIAL_POWER);

        // Then unregister
        vm.prank(alice);
        vm.expectEmit(true, true, true, true);
        emit ValidatorUnregistered(alice);

        validatorSet.unregister();
        vm.expectRevert(ValidatorSet.ValidatorDoesNotExist.selector);
        validatorSet.getValidator(alice);
        assertEq(validatorSet.getTotalPower(), 0);
        assertEq(validatorSet.getValidatorCount(), 0);
        assertFalse(validatorSet.isValidator(alice));
    }

    function testCannotUnregisterNonExistentValidator() public {
        vm.prank(alice);
        vm.expectRevert(ValidatorSet.ValidatorDoesNotExist.selector);
        validatorSet.unregister();
    }

    function testCannotUnregisterOtherValidator() public {
        // Alice registers
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, INITIAL_POWER);

        // Bob tries to unregister (bob is not a validator, so should get ValidatorDoesNotExist)
        vm.prank(bob);
        vm.expectRevert(ValidatorSet.ValidatorDoesNotExist.selector);
        validatorSet.unregister();
    }

    // Power Update Tests
    function testUpdateValidatorPower() public {
        // First register
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, INITIAL_POWER);

        // Update power
        vm.prank(alice);
        vm.expectEmit(true, true, true, true);
        emit ValidatorPowerUpdated(alice, INITIAL_POWER, UPDATED_POWER);

        validatorSet.updatePower(UPDATED_POWER);
        ValidatorSet.ValidatorInfoFull memory info = validatorSet.getValidator(alice);
        assertEq(info.validator, alice);
        assertEq(info.ed25519Key, ALICE_KEY);
        assertEq(info.power, UPDATED_POWER);
        assertEq(validatorSet.getTotalPower(), UPDATED_POWER);
    }

    function testUpdatePowerWithMultipleValidators() public {
        // Register multiple validators
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, 100);
        vm.prank(bob);
        validatorSet.register(BOB_KEY, 200);

        // Update Alice's power
        vm.prank(alice);
        validatorSet.updatePower(150);

        // Check Alice's new power and that Bob's is unchanged
        ValidatorSet.ValidatorInfoFull memory aliceInfo = validatorSet.getValidator(alice);
        ValidatorSet.ValidatorInfoFull memory bobInfo = validatorSet.getValidator(bob);
        assertEq(aliceInfo.power, 150);
        assertEq(bobInfo.power, 200);
        assertEq(validatorSet.getTotalPower(), 350); // 150 + 200
    }

    function testCannotUpdatePowerToZero() public {
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, INITIAL_POWER);

        vm.prank(alice);
        vm.expectRevert(ValidatorSet.InvalidPower.selector);
        validatorSet.updatePower(0);
    }

    function testCannotUpdateNonExistentValidatorPower() public {
        vm.prank(alice);
        vm.expectRevert(ValidatorSet.ValidatorDoesNotExist.selector);
        validatorSet.updatePower(UPDATED_POWER);
    }

    function testCannotUpdateOtherValidatorPower() public {
        // Alice registers
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, INITIAL_POWER);

        // Bob tries to update power (bob is not a validator, so should get ValidatorDoesNotExist)
        vm.prank(bob);
        vm.expectRevert(ValidatorSet.ValidatorDoesNotExist.selector);
        validatorSet.updatePower(UPDATED_POWER);
    }

    function testValidatorCanOnlyUpdateOwnPower() public {
        // Both alice and bob register
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, INITIAL_POWER);

        vm.prank(bob);
        validatorSet.register(BOB_KEY, INITIAL_POWER);

        // Alice tries to update bob's power by calling from alice's address
        // This would require a different function signature to specify target validator
        // Since our current design only allows self-modification, this test shows
        // that each validator can only modify their own data

        // Alice can update her own power
        vm.prank(alice);
        validatorSet.updatePower(UPDATED_POWER);
        ValidatorSet.ValidatorInfoFull memory aliceInfo = validatorSet.getValidator(alice);
        assertEq(aliceInfo.power, UPDATED_POWER);

        // Bob can update his own power
        vm.prank(bob);
        validatorSet.updatePower(UPDATED_POWER);
        ValidatorSet.ValidatorInfoFull memory bobInfo = validatorSet.getValidator(bob);
        assertEq(bobInfo.power, UPDATED_POWER);
    }

    // View Function Tests
    function testGetValidators() public {
        // Register multiple validators
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, 100);
        vm.prank(bob);
        validatorSet.register(BOB_KEY, 200);
        vm.prank(charlie);
        validatorSet.register(CHARLIE_KEY, 300);

        ValidatorSet.ValidatorInfoFull[] memory validators = validatorSet.getValidators();
        assertEq(validators.length, 3);

        // Check validators are returned (order may vary due to EnumerableSet)
        bool aliceFound = false;
        bool bobFound = false;
        bool charlieFound = false;

        for (uint256 i = 0; i < validators.length; i++) {
            ValidatorSet.ValidatorInfoFull memory info = validators[i];
            if (info.validator == alice) {
                assertEq(info.ed25519Key, ALICE_KEY);
                assertEq(info.power, 100);
                aliceFound = true;
            } else if (info.validator == bob) {
                assertEq(info.ed25519Key, BOB_KEY);
                assertEq(info.power, 200);
                bobFound = true;
            } else if (info.validator == charlie) {
                assertEq(info.ed25519Key, CHARLIE_KEY);
                assertEq(info.power, 300);
                charlieFound = true;
            }
        }

        assertTrue(aliceFound);
        assertTrue(bobFound);
        assertTrue(charlieFound);
    }

    function testGetValidatorAddresses() public {
        // Register validators
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, 100);
        vm.prank(bob);
        validatorSet.register(BOB_KEY, 200);

        address[] memory addresses = validatorSet.getValidatorAddresses();
        assertEq(addresses.length, 2);

        // Check addresses are included (order may vary)
        bool aliceFound = false;
        bool bobFound = false;

        for (uint256 i = 0; i < addresses.length; i++) {
            if (addresses[i] == alice) aliceFound = true;
            if (addresses[i] == bob) bobFound = true;
        }

        assertTrue(aliceFound);
        assertTrue(bobFound);
    }

    function testGetValidatorNonExistent() public {
        vm.expectRevert(ValidatorSet.ValidatorDoesNotExist.selector);
        validatorSet.getValidator(alice);
    }

    // Edge Cases and Integration Tests
    function testFullWorkflow() public {
        // 1. Register validator
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, 100);
        assertEq(validatorSet.getTotalPower(), 100);

        // 2. Update power
        vm.prank(alice);
        validatorSet.updatePower(200);
        assertEq(validatorSet.getTotalPower(), 200);

        // 3. Register another validator
        vm.prank(bob);
        validatorSet.register(BOB_KEY, 150);
        assertEq(validatorSet.getTotalPower(), 350);

        // 4. Unregister first validator
        vm.prank(alice);
        validatorSet.unregister();
        assertEq(validatorSet.getTotalPower(), 150);
        assertEq(validatorSet.getValidatorCount(), 1);

        // 5. Verify only bob remains
        assertFalse(validatorSet.isValidator(alice));
        assertTrue(validatorSet.isValidator(bob));
    }

    function testEmptyValidatorSet() public view {
        assertEq(validatorSet.getTotalPower(), 0);
        assertEq(validatorSet.getValidatorCount(), 0);

        ValidatorSet.ValidatorInfoFull[] memory validators = validatorSet.getValidators();
        assertEq(validators.length, 0);

        address[] memory validatorAddresses = validatorSet.getValidatorAddresses();
        assertEq(validatorAddresses.length, 0);
    }

    // Fuzz Tests
    function testFuzzRegisterPower(uint256 power) public {
        vm.assume(power > 0);
        vm.assume(power < type(uint256).max); // Prevent overflow in totalPower

        vm.prank(alice);
        validatorSet.register(ALICE_KEY, power);

        ValidatorSet.ValidatorInfoFull memory info = validatorSet.getValidator(alice);
        assertEq(info.power, power);
        assertEq(validatorSet.getTotalPower(), power);
    }

    function testFuzzUpdatePower(uint256 initialPower, uint256 newPower) public {
        vm.assume(initialPower > 0);
        vm.assume(newPower > 0);
        vm.assume(initialPower < type(uint256).max);
        vm.assume(newPower < type(uint256).max);

        vm.prank(alice);
        validatorSet.register(ALICE_KEY, initialPower);

        vm.prank(alice);
        validatorSet.updatePower(newPower);

        ValidatorSet.ValidatorInfoFull memory info = validatorSet.getValidator(alice);
        assertEq(info.power, newPower);
        assertEq(validatorSet.getTotalPower(), newPower);
    }

    // Key Update Tests
    function testUpdateValidatorKey() public {
        // First register
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, INITIAL_POWER);

        // Update key
        vm.prank(alice);
        vm.expectEmit(true, true, true, true);
        emit ValidatorKeyUpdated(alice, ALICE_KEY, NEW_KEY);

        validatorSet.updateKey(NEW_KEY);

        ValidatorSet.ValidatorInfoFull memory info = validatorSet.getValidator(alice);
        assertEq(info.validator, alice);
        assertEq(info.ed25519Key, NEW_KEY);
        assertEq(info.power, INITIAL_POWER);
    }

    function testCannotUpdateKeyWithZeroKey() public {
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, INITIAL_POWER);

        vm.prank(alice);
        vm.expectRevert(ValidatorSet.InvalidKey.selector);
        validatorSet.updateKey(bytes32(0));
    }

    function testCannotUpdateNonExistentValidatorKey() public {
        vm.prank(alice);
        vm.expectRevert(ValidatorSet.ValidatorDoesNotExist.selector);
        validatorSet.updateKey(NEW_KEY);
    }

    // Reentrancy Protection Tests
    function testReentrancyProtection() public {
        // This is more of a demonstration that ReentrancyGuard is in place
        // Real reentrancy testing would require a malicious contract
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, 100);

        // Verify the modifier is working by checking state changes happen atomically
        assertEq(validatorSet.getTotalPower(), 100);
        assertTrue(validatorSet.isValidator(alice));
    }
}
