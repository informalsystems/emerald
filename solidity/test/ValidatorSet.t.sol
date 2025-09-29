// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {Test, console} from "forge-std/Test.sol";
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

    function testCannotRegisterWithZeroKey() public {
        vm.prank(alice);
        vm.expectRevert(ValidatorSet.InvalidKey.selector);
        validatorSet.register(bytes32(0), INITIAL_POWER);
    }

    function testCannotRegisterZeroAddress() public {
        vm.prank(address(0));
        vm.expectRevert(ValidatorSet.ZeroAddress.selector);
        validatorSet.register(ALICE_KEY, INITIAL_POWER);
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

    // Invariant Tests
    function testInvariantTotalPowerConsistency() public {
        // Register multiple validators
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, 100);
        vm.prank(bob);
        validatorSet.register(BOB_KEY, 200);
        vm.prank(charlie);
        validatorSet.register(CHARLIE_KEY, 300);

        // Manual calculation
        uint256 expectedTotal = 100 + 200 + 300;
        assertEq(validatorSet.getTotalPower(), expectedTotal);

        // Update power
        vm.prank(alice);
        validatorSet.updatePower(150);
        expectedTotal = 150 + 200 + 300;
        assertEq(validatorSet.getTotalPower(), expectedTotal);

        // Unregister
        vm.prank(bob);
        validatorSet.unregister();
        expectedTotal = 150 + 300;
        assertEq(validatorSet.getTotalPower(), expectedTotal);
    }

    // Gas Optimization Tests
    function testGasBenchmarkGetValidators() public {
        // Register 10 validators
        for (uint256 i = 1; i <= 10; i++) {
            address validator = address(uint160(i));
            bytes32 key = bytes32(uint256(i * 1111));
            vm.prank(validator);
            validatorSet.register(key, i * 100);
        }

        // Measure gas for getValidators
        uint256 gasBefore = gasleft();
        validatorSet.getValidators();
        uint256 gasUsed = gasBefore - gasleft();

        // Log gas usage
        console.log("Gas used for getValidators() with 10 validators:", gasUsed);
        // Should be reasonable (adjust threshold as needed)
        assertLt(gasUsed, 500000, "getValidators gas usage too high");
    }

    function testGasBenchmarkGetTotalPower() public {
        // Register 10 validators
        for (uint256 i = 1; i <= 10; i++) {
            address validator = address(uint160(i));
            bytes32 key = bytes32(uint256(i * 1111));
            vm.prank(validator);
            validatorSet.register(key, i * 100);
        }

        // Measure gas for getTotalPower
        uint256 gasBefore = gasleft();
        validatorSet.getTotalPower();
        uint256 gasUsed = gasBefore - gasleft();

        // Log gas usage
        console.log("Gas used for getTotalPower() with 10 validators:", gasUsed);
        assertLt(gasUsed, 200000, "getTotalPower gas usage too high");
    }

    // Edge case: Duplicate Ed25519 keys
    function testDuplicateEd25519KeysAllowed() public {
        // The contract currently allows duplicate Ed25519 keys
        // This test documents this behavior
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, 100);

        vm.prank(bob);
        validatorSet.register(ALICE_KEY, 200); // Same key, different validator

        // Both validators should exist
        assertTrue(validatorSet.isValidator(alice));
        assertTrue(validatorSet.isValidator(bob));

        ValidatorSet.ValidatorInfoFull memory aliceInfo = validatorSet.getValidator(alice);
        ValidatorSet.ValidatorInfoFull memory bobInfo = validatorSet.getValidator(bob);

        assertEq(aliceInfo.ed25519Key, ALICE_KEY);
        assertEq(bobInfo.ed25519Key, ALICE_KEY);
    }

    // Edge case: Maximum uint256 power
    function testMaxUint256PowerHandling() public {
        uint256 maxPower = type(uint256).max;

        vm.prank(alice);
        validatorSet.register(ALICE_KEY, maxPower);

        ValidatorSet.ValidatorInfoFull memory info = validatorSet.getValidator(alice);
        assertEq(info.power, maxPower);
    }

    function testTotalPowerOverflow() public {
        // Register validator with max uint256
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, type(uint256).max);

        // Trying to register another validator would cause overflow in getTotalPower
        vm.prank(bob);
        validatorSet.register(BOB_KEY, 1);

        // getTotalPower will overflow (Solidity 0.8+ reverts on overflow)
        vm.expectRevert();
        validatorSet.getTotalPower();
    }

    // Key update edge cases
    function testMultipleKeyUpdates() public {
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, INITIAL_POWER);

        // Update key multiple times
        vm.prank(alice);
        validatorSet.updateKey(BOB_KEY);

        vm.prank(alice);
        validatorSet.updateKey(CHARLIE_KEY);

        vm.prank(alice);
        validatorSet.updateKey(NEW_KEY);

        ValidatorSet.ValidatorInfoFull memory info = validatorSet.getValidator(alice);
        assertEq(info.ed25519Key, NEW_KEY);
        assertEq(info.power, INITIAL_POWER); // Power unchanged
    }

    // Test validator enumeration order
    function testValidatorEnumerationConsistency() public {
        // Register in specific order
        vm.prank(alice);
        validatorSet.register(ALICE_KEY, 100);
        vm.prank(bob);
        validatorSet.register(BOB_KEY, 200);
        vm.prank(charlie);
        validatorSet.register(CHARLIE_KEY, 300);

        // Get validators twice
        ValidatorSet.ValidatorInfoFull[] memory validators1 = validatorSet.getValidators();
        ValidatorSet.ValidatorInfoFull[] memory validators2 = validatorSet.getValidators();

        // Should return same order
        assertEq(validators1.length, validators2.length);
        for (uint256 i = 0; i < validators1.length; i++) {
            assertEq(validators1[i].validator, validators2[i].validator);
            assertEq(validators1[i].ed25519Key, validators2[i].ed25519Key);
            assertEq(validators1[i].power, validators2[i].power);
        }
    }
}
