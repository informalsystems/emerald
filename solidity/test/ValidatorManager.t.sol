// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {Test} from "forge-std/Test.sol";
import {ValidatorManager} from "../src/ValidatorManager.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

contract ValidatorManagerTest is Test {
    ValidatorManager internal validatorManager;

    uint256 internal constant ALICE_KEY_X = 0xA11CE;
    uint256 internal constant ALICE_KEY_Y = 0x1;
    uint256 internal constant BOB_KEY_X = 0xB0B;
    uint256 internal constant BOB_KEY_Y = 0x2;
    uint256 internal constant COFFEE_KEY_X = 0xC0FFEE;
    uint256 internal constant COFFEE_KEY_Y = 0x3;
    uint64 internal constant INITIAL_POWER = 100;
    uint64 internal constant UPDATED_POWER = 200;
    uint64 internal constant SECOND_POWER = 150;
    uint64 internal constant THIRD_POWER = 50;
    address internal constant NON_OWNER = address(0xBEEF);
    address internal constant NEW_OWNER = address(0xCAFE);

    uint256 internal constant DERIVED_PUBLIC_KEY_X = 0x8318535b54105d4a7aae60c08fc45f9687181b4fdfc625bd1a753fa7397fed75;
    uint256 internal constant DERIVED_PUBLIC_KEY_Y = 0x3547f11ca8696646f2f3acb08e31016afac23e630c5d11f59f61fef57b0d2aa5;
    uint256 internal constant DERIVED_PRIVATE_KEY = 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80;

    function aliceKey() internal pure returns (ValidatorManager.Secp256k1Key memory) {
        return ValidatorManager.Secp256k1Key({x: ALICE_KEY_X, y: ALICE_KEY_Y});
    }

    function bobKey() internal pure returns (ValidatorManager.Secp256k1Key memory) {
        return ValidatorManager.Secp256k1Key({x: BOB_KEY_X, y: BOB_KEY_Y});
    }

    function coffeeKey() internal pure returns (ValidatorManager.Secp256k1Key memory) {
        return ValidatorManager.Secp256k1Key({x: COFFEE_KEY_X, y: COFFEE_KEY_Y});
    }

    function zeroKey() internal pure returns (ValidatorManager.Secp256k1Key memory) {
        return ValidatorManager.Secp256k1Key({x: 0, y: 0});
    }

    function validatorKeyId(ValidatorManager.Secp256k1Key memory key) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(key.x, key.y));
    }

    function keysEqual(ValidatorManager.Secp256k1Key memory a, ValidatorManager.Secp256k1Key memory b)
        internal
        pure
        returns (bool)
    {
        return a.x == b.x && a.y == b.y;
    }

    function assertKeyEq(ValidatorManager.Secp256k1Key memory actual, ValidatorManager.Secp256k1Key memory expected)
        internal
        pure
    {
        require(keysEqual(actual, expected), "validator key mismatch");
    }

    event ValidatorRegistered(bytes32 indexed validatorKeyId, ValidatorManager.Secp256k1Key validatorKey, uint64 power);
    event ValidatorUnregistered(bytes32 indexed validatorKeyId, ValidatorManager.Secp256k1Key validatorKey);
    event ValidatorPowerUpdated(
        bytes32 indexed validatorKeyId, ValidatorManager.Secp256k1Key validatorKey, uint64 oldPower, uint64 newPower
    );

    function setUp() public {
        validatorManager = new ValidatorManager();
    }

    function testOwnerCanRegisterValidator() public {
        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(validatorKeyId(aliceKey()), aliceKey(), INITIAL_POWER);

        validatorManager.register(aliceKey(), INITIAL_POWER);

        ValidatorManager.ValidatorInfo memory info = validatorManager.getValidator(aliceKey());
        assertKeyEq(info.validatorKey, aliceKey());
        assertEq(info.power, INITIAL_POWER);
        assertEq(validatorManager.getValidatorCount(), 1);
        assertEq(validatorManager.getTotalPower(), INITIAL_POWER);
        assertTrue(validatorManager.isValidator(aliceKey()));

        ValidatorManager.Secp256k1Key[] memory keys = validatorManager.getValidatorKeys();
        assertEq(keys.length, 1);
        assertKeyEq(keys[0], aliceKey());
    }

    function testOwnerCanRegisterSetOfValidators() public {
        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(validatorKeyId(aliceKey()), aliceKey(), INITIAL_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(validatorKeyId(bobKey()), bobKey(), SECOND_POWER);

        ValidatorManager.ValidatorInfo[] memory addValidators = new ValidatorManager.ValidatorInfo[](2);
        addValidators[0] = ValidatorManager.ValidatorInfo({validatorKey: aliceKey(), power: INITIAL_POWER});
        addValidators[1] = ValidatorManager.ValidatorInfo({validatorKey: bobKey(), power: SECOND_POWER});

        validatorManager.registerSet(addValidators);

        assertEq(validatorManager.getValidatorCount(), 2);
        assertEq(validatorManager.getTotalPower(), INITIAL_POWER + SECOND_POWER);

        ValidatorManager.ValidatorInfo memory aliceInfo = validatorManager.getValidator(aliceKey());
        assertKeyEq(aliceInfo.validatorKey, aliceKey());
        assertEq(aliceInfo.power, INITIAL_POWER);
        assertTrue(validatorManager.isValidator(aliceKey()));

        ValidatorManager.ValidatorInfo memory bobInfo = validatorManager.getValidator(bobKey());
        assertKeyEq(bobInfo.validatorKey, bobKey());
        assertEq(bobInfo.power, SECOND_POWER);
        assertTrue(validatorManager.isValidator(bobKey()));

        ValidatorManager.Secp256k1Key[] memory retrievedKeys = validatorManager.getValidatorKeys();
        assertEq(retrievedKeys.length, 2);
        assertKeyEq(retrievedKeys[0], aliceKey());
        assertKeyEq(retrievedKeys[1], bobKey());
    }

    function testNonOwnerCannotRegisterValidator() public {
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.register(aliceKey(), INITIAL_POWER);
    }

    function testRegisterRejectsInvalidInputs() public {
        vm.expectRevert(ValidatorManager.InvalidKey.selector);
        validatorManager.register(zeroKey(), INITIAL_POWER);

        vm.expectRevert(ValidatorManager.InvalidPower.selector);
        validatorManager.register(aliceKey(), 0);
    }

    function testRegisterRejectsDuplicateKey() public {
        validatorManager.register(aliceKey(), INITIAL_POWER);

        vm.expectRevert(ValidatorManager.ValidatorAlreadyExists.selector);
        validatorManager.register(aliceKey(), SECOND_POWER);
    }

    function testOwnerCanUpdatePower() public {
        validatorManager.register(aliceKey(), INITIAL_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorPowerUpdated(validatorKeyId(aliceKey()), aliceKey(), INITIAL_POWER, UPDATED_POWER);

        validatorManager.updatePower(aliceKey(), UPDATED_POWER);

        ValidatorManager.ValidatorInfo memory info = validatorManager.getValidator(aliceKey());
        assertEq(info.power, UPDATED_POWER);
        assertEq(validatorManager.getTotalPower(), UPDATED_POWER);
    }

    function testNonOwnerCannotUpdatePower() public {
        validatorManager.register(aliceKey(), INITIAL_POWER);

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.updatePower(aliceKey(), UPDATED_POWER);
    }

    function testUpdatePowerRequiresExistingValidator() public {
        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.updatePower(aliceKey(), UPDATED_POWER);
    }

    function testOwnerCanUnregisterValidator() public {
        validatorManager.register(aliceKey(), INITIAL_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorUnregistered(validatorKeyId(aliceKey()), aliceKey());

        validatorManager.unregister(aliceKey());

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(aliceKey());
        assertEq(validatorManager.getValidatorCount(), 0);
        assertEq(validatorManager.getTotalPower(), 0);
        assertFalse(validatorManager.isValidator(aliceKey()));
    }

    function testOwnerCanUnregisterSetOfValidators() public {
        validatorManager.register(aliceKey(), INITIAL_POWER);
        validatorManager.register(bobKey(), SECOND_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorUnregistered(validatorKeyId(aliceKey()), aliceKey());

        vm.expectEmit(true, false, false, true);
        emit ValidatorUnregistered(validatorKeyId(bobKey()), bobKey());

        ValidatorManager.Secp256k1Key[] memory keys = new ValidatorManager.Secp256k1Key[](2);
        keys[0] = aliceKey();
        keys[1] = bobKey();

        validatorManager.unregisterSet(keys);

        assertEq(validatorManager.getValidatorCount(), 0);
        assertEq(validatorManager.getTotalPower(), 0);

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(aliceKey());
        assertFalse(validatorManager.isValidator(aliceKey()));

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(bobKey());
        assertFalse(validatorManager.isValidator(bobKey()));
    }

    function testNonOwnerCannotUnregisterValidator() public {
        validatorManager.register(aliceKey(), INITIAL_POWER);

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.unregister(aliceKey());
    }

    function testUnregisterRequiresExistingValidator() public {
        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.unregister(aliceKey());
    }

    function testGetValidatorRevertsWhenMissing() public {
        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(aliceKey());
    }

    function testGetValidatorsAggregatesAllEntries() public {
        validatorManager.register(aliceKey(), INITIAL_POWER);
        validatorManager.register(bobKey(), SECOND_POWER);

        ValidatorManager.ValidatorInfo[] memory validators = validatorManager.getValidators();
        assertEq(validators.length, 2);

        uint256 totalPower;
        bool sawAlice;
        bool sawBob;
        for (uint256 i = 0; i < validators.length; i++) {
            totalPower += validators[i].power;
            if (keysEqual(validators[i].validatorKey, aliceKey())) {
                assertEq(validators[i].power, INITIAL_POWER);
                sawAlice = true;
            } else if (keysEqual(validators[i].validatorKey, bobKey())) {
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

        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(validatorKeyId(aliceKey()), aliceKey(), INITIAL_POWER);
        vm.prank(NEW_OWNER);
        validatorManager.register(aliceKey(), INITIAL_POWER);

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(this)));
        validatorManager.register(bobKey(), SECOND_POWER);
    }

    function testRenounceOwnershipLocksMutations() public {
        validatorManager.renounceOwnership();
        assertEq(validatorManager.owner(), address(0));

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(this)));
        validatorManager.register(aliceKey(), INITIAL_POWER);

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.updatePower(aliceKey(), UPDATED_POWER);
    }

    function testOwnerCanAddAndRemoveValidators() public {
        validatorManager.register(aliceKey(), INITIAL_POWER);
        validatorManager.register(bobKey(), SECOND_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(validatorKeyId(coffeeKey()), coffeeKey(), THIRD_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorUnregistered(validatorKeyId(aliceKey()), aliceKey());

        vm.expectEmit(true, false, false, true);
        emit ValidatorUnregistered(validatorKeyId(bobKey()), bobKey());

        ValidatorManager.ValidatorInfo[] memory addValidators = new ValidatorManager.ValidatorInfo[](1);
        addValidators[0] = ValidatorManager.ValidatorInfo({validatorKey: coffeeKey(), power: THIRD_POWER});

        ValidatorManager.Secp256k1Key[] memory removeKeys = new ValidatorManager.Secp256k1Key[](2);
        removeKeys[0] = aliceKey();
        removeKeys[1] = bobKey();

        validatorManager.addAndRemove(addValidators, removeKeys);

        assertEq(validatorManager.getValidatorCount(), 1);
        assertEq(validatorManager.getTotalPower(), THIRD_POWER);

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(aliceKey());
        assertFalse(validatorManager.isValidator(aliceKey()));

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(bobKey());
        assertFalse(validatorManager.isValidator(bobKey()));

        ValidatorManager.ValidatorInfo memory info = validatorManager.getValidator(coffeeKey());
        assertKeyEq(info.validatorKey, coffeeKey());
        assertEq(info.power, THIRD_POWER);
        assertTrue(validatorManager.isValidator(coffeeKey()));

        ValidatorManager.Secp256k1Key[] memory keys = validatorManager.getValidatorKeys();
        assertEq(keys.length, 1);
        assertKeyEq(keys[0], coffeeKey());
    }

    function mnemonicDerivedKey() internal pure returns (ValidatorManager.Secp256k1Key memory) {
        return ValidatorManager.Secp256k1Key({x: DERIVED_PUBLIC_KEY_X, y: DERIVED_PUBLIC_KEY_Y});
    }

    function testValidatorKeyIdMatchesAddressDerivedFromPrivateKey() public pure {
        ValidatorManager.Secp256k1Key memory key = mnemonicDerivedKey();
        bytes32 keyId = keccak256(abi.encodePacked(key.x, key.y));
        address derived = address(uint160(uint256(keyId)));
        address expected = vm.addr(DERIVED_PRIVATE_KEY);
        assertEq(derived, expected);
    }
}
