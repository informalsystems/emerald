// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {Test} from "forge-std/Test.sol";
import {ValidatorManager} from "../src/ValidatorManager.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

contract ValidatorManagerTest is Test {
    ValidatorManager internal validatorManager;

    uint256 internal constant ALICE_KEY = 0xA11CE;
    uint256 internal constant BOB_KEY = 0xB0B;
    uint256 internal constant COFFEE_KEY = 0xC0FFEE;
    uint256 internal constant INITIAL_POWER = 100;
    uint256 internal constant UPDATED_POWER = 200;
    uint256 internal constant SECOND_POWER = 150;
    uint256 internal constant THIRD_POWER = 50;
    address internal constant NON_OWNER = address(0xBEEF);
    address internal constant NEW_OWNER = address(0xCAFE);

    event ValidatorRegistered(uint256 indexed validatorKey, uint256 power);
    event ValidatorUnregistered(uint256 indexed validatorKey);
    event ValidatorPowerUpdated(uint256 indexed validatorKey, uint256 oldPower, uint256 newPower);

    function setUp() public {
        validatorManager = new ValidatorManager();
    }

    function testOwnerCanRegisterValidator() public {
        vm.expectEmit(true, true, true, true);
        emit ValidatorRegistered(ALICE_KEY, INITIAL_POWER);

        validatorManager.register(ALICE_KEY, INITIAL_POWER);

        ValidatorManager.ValidatorInfo memory info = validatorManager.getValidator(ALICE_KEY);
        assertEq(info.validatorKey, ALICE_KEY);
        assertEq(info.power, INITIAL_POWER);
        assertEq(validatorManager.getValidatorCount(), 1);
        assertEq(validatorManager.getTotalPower(), INITIAL_POWER);
        assertTrue(validatorManager.isValidator(ALICE_KEY));

        uint256[] memory keys = validatorManager.getValidatorKeys();
        assertEq(keys.length, 1);
        assertEq(keys[0], ALICE_KEY);
    }

    function testOwnerCanRegisterSetOfValidators() public {
        vm.expectEmit(true, true, true, true);
        emit ValidatorRegistered(ALICE_KEY, INITIAL_POWER);

        vm.expectEmit(true, true, true, true);
        emit ValidatorRegistered(BOB_KEY, SECOND_POWER);

        ValidatorManager.ValidatorInfo[] memory addValidators = new ValidatorManager.ValidatorInfo[](2);
        addValidators[0] = ValidatorManager.ValidatorInfo({validatorKey: ALICE_KEY, power: INITIAL_POWER});
        addValidators[1] = ValidatorManager.ValidatorInfo({validatorKey: BOB_KEY, power: SECOND_POWER});

        validatorManager.registerSet(addValidators);

        assertEq(validatorManager.getValidatorCount(), 2);
        assertEq(validatorManager.getTotalPower(), INITIAL_POWER + SECOND_POWER);

        ValidatorManager.ValidatorInfo memory aliceInfo = validatorManager.getValidator(ALICE_KEY);
        assertEq(aliceInfo.validatorKey, ALICE_KEY);
        assertEq(aliceInfo.power, INITIAL_POWER);
        assertTrue(validatorManager.isValidator(ALICE_KEY));

        ValidatorManager.ValidatorInfo memory bobInfo = validatorManager.getValidator(BOB_KEY);
        assertEq(bobInfo.validatorKey, BOB_KEY);
        assertEq(bobInfo.power, SECOND_POWER);
        assertTrue(validatorManager.isValidator(BOB_KEY));

        uint256[] memory retrievedKeys = validatorManager.getValidatorKeys();
        assertEq(retrievedKeys.length, 2);
        assertEq(retrievedKeys[0], ALICE_KEY);
        assertEq(retrievedKeys[1], BOB_KEY);
    }

    function testNonOwnerCannotRegisterValidator() public {
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.register(ALICE_KEY, INITIAL_POWER);
    }

    function testRegisterRejectsInvalidInputs() public {
        vm.expectRevert(ValidatorManager.InvalidKey.selector);
        validatorManager.register(0, INITIAL_POWER);

        vm.expectRevert(ValidatorManager.InvalidPower.selector);
        validatorManager.register(ALICE_KEY, 0);
    }

    function testRegisterRejectsDuplicateKey() public {
        validatorManager.register(ALICE_KEY, INITIAL_POWER);

        vm.expectRevert(ValidatorManager.ValidatorAlreadyExists.selector);
        validatorManager.register(ALICE_KEY, SECOND_POWER);
    }

    function testOwnerCanUpdatePower() public {
        validatorManager.register(ALICE_KEY, INITIAL_POWER);

        vm.expectEmit(true, true, true, true);
        emit ValidatorPowerUpdated(ALICE_KEY, INITIAL_POWER, UPDATED_POWER);

        validatorManager.updatePower(ALICE_KEY, UPDATED_POWER);

        ValidatorManager.ValidatorInfo memory info = validatorManager.getValidator(ALICE_KEY);
        assertEq(info.power, UPDATED_POWER);
        assertEq(validatorManager.getTotalPower(), UPDATED_POWER);
    }

    function testNonOwnerCannotUpdatePower() public {
        validatorManager.register(ALICE_KEY, INITIAL_POWER);

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.updatePower(ALICE_KEY, UPDATED_POWER);
    }

    function testUpdatePowerRequiresExistingValidator() public {
        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.updatePower(ALICE_KEY, UPDATED_POWER);
    }

    function testOwnerCanUnregisterValidator() public {
        validatorManager.register(ALICE_KEY, INITIAL_POWER);

        vm.expectEmit(true, true, true, true);
        emit ValidatorUnregistered(ALICE_KEY);

        validatorManager.unregister(ALICE_KEY);

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(ALICE_KEY);
        assertEq(validatorManager.getValidatorCount(), 0);
        assertEq(validatorManager.getTotalPower(), 0);
        assertFalse(validatorManager.isValidator(ALICE_KEY));
    }

    function testOwnerCanUnregisterSetOfValidators() public {
        validatorManager.register(ALICE_KEY, INITIAL_POWER);
        validatorManager.register(BOB_KEY, SECOND_POWER);

        vm.expectEmit(true, true, true, true);
        emit ValidatorUnregistered(ALICE_KEY);

        vm.expectEmit(true, true, true, true);
        emit ValidatorUnregistered(BOB_KEY);

        uint256[] memory keys = new uint256[](2);
        keys[0] = ALICE_KEY;
        keys[1] = BOB_KEY;

        validatorManager.unregisterSet(keys);

        assertEq(validatorManager.getValidatorCount(), 0);
        assertEq(validatorManager.getTotalPower(), 0);

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(ALICE_KEY);
        assertFalse(validatorManager.isValidator(ALICE_KEY));

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(BOB_KEY);
        assertFalse(validatorManager.isValidator(BOB_KEY));
    }

    function testNonOwnerCannotUnregisterValidator() public {
        validatorManager.register(ALICE_KEY, INITIAL_POWER);

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.unregister(ALICE_KEY);
    }

    function testUnregisterRequiresExistingValidator() public {
        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.unregister(ALICE_KEY);
    }

    function testGetValidatorRevertsWhenMissing() public {
        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(ALICE_KEY);
    }

    function testGetValidatorsAggregatesAllEntries() public {
        validatorManager.register(ALICE_KEY, INITIAL_POWER);
        validatorManager.register(BOB_KEY, SECOND_POWER);

        ValidatorManager.ValidatorInfo[] memory validators = validatorManager.getValidators();
        assertEq(validators.length, 2);

        uint256 totalPower;
        bool sawAlice;
        bool sawBob;
        for (uint256 i = 0; i < validators.length; i++) {
            totalPower += validators[i].power;
            if (validators[i].validatorKey == ALICE_KEY) {
                assertEq(validators[i].power, INITIAL_POWER);
                sawAlice = true;
            } else if (validators[i].validatorKey == BOB_KEY) {
                assertEq(validators[i].power, SECOND_POWER);
                sawBob = true;
            }
        }

        assertTrue(sawAlice && sawBob);
        assertEq(totalPower, validatorManager.getTotalPower());
        assertEq(validatorManager.getValidatorCount(), 2);
    }

    function testTransferOwnershipGivesControlToNewOwner() public {
        validatorManager.transferOwnership(NEW_OWNER);
        assertEq(validatorManager.owner(), NEW_OWNER);

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.transferOwnership(address(0xBAD));

        vm.expectEmit(true, true, true, true);
        emit ValidatorRegistered(ALICE_KEY, INITIAL_POWER);
        vm.prank(NEW_OWNER);
        validatorManager.register(ALICE_KEY, INITIAL_POWER);

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(this)));
        validatorManager.register(BOB_KEY, SECOND_POWER);
    }

    function testRenounceOwnershipLocksMutations() public {
        validatorManager.renounceOwnership();
        assertEq(validatorManager.owner(), address(0));

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(this)));
        validatorManager.register(ALICE_KEY, INITIAL_POWER);

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.updatePower(ALICE_KEY, UPDATED_POWER);
    }

    function testOwnerCanAddAndRemoveValidators() public {
        validatorManager.register(ALICE_KEY, INITIAL_POWER);
        validatorManager.register(BOB_KEY, SECOND_POWER);

        vm.expectEmit(true, true, true, true);
        emit ValidatorRegistered(COFFEE_KEY, THIRD_POWER);

        vm.expectEmit(true, true, true, true);
        emit ValidatorUnregistered(ALICE_KEY);

        vm.expectEmit(true, true, true, true);
        emit ValidatorUnregistered(BOB_KEY);

        ValidatorManager.ValidatorInfo[] memory addValidators = new ValidatorManager.ValidatorInfo[](1);
        addValidators[0] = ValidatorManager.ValidatorInfo({validatorKey: COFFEE_KEY, power: THIRD_POWER});

        uint256[] memory removeKeys = new uint256[](2);
        removeKeys[0] = ALICE_KEY;
        removeKeys[1] = BOB_KEY;

        validatorManager.addAndRemove(addValidators, removeKeys);

        assertEq(validatorManager.getValidatorCount(), 1);
        assertEq(validatorManager.getTotalPower(), THIRD_POWER);

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(ALICE_KEY);
        assertFalse(validatorManager.isValidator(ALICE_KEY));

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(BOB_KEY);
        assertFalse(validatorManager.isValidator(BOB_KEY));

        ValidatorManager.ValidatorInfo memory info = validatorManager.getValidator(COFFEE_KEY);
        assertEq(info.validatorKey, COFFEE_KEY);
        assertEq(info.power, THIRD_POWER);
        assertTrue(validatorManager.isValidator(COFFEE_KEY));

        uint256[] memory keys = validatorManager.getValidatorKeys();
        assertEq(keys.length, 1);
        assertEq(keys[0], COFFEE_KEY);
    }
}
