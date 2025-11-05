// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {Test} from "forge-std/Test.sol";
import {ValidatorManager} from "../src/ValidatorManager.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

contract ValidatorManagerTest is Test {
    ValidatorManager internal validatorManager;

    uint256 internal constant ALICE_KEY_X = 0x8318535b54105d4a7aae60c08fc45f9687181b4fdfc625bd1a753fa7397fed75;
    uint256 internal constant ALICE_KEY_Y = 0x3547f11ca8696646f2f3acb08e31016afac23e630c5d11f59f61fef57b0d2aa5;
    uint256 internal constant BOB_KEY_X = 0xba5734d8f7091719471e7f7ed6b9df170dc70cc661ca05e688601ad984f068b0;
    uint256 internal constant BOB_KEY_Y = 0xd67351e5f06073092499336ab0839ef8a521afd334e53807205fa2f08eec74f4;
    uint256 internal constant COFFEE_KEY_X = 0x9d9031e97dd78ff8c15aa86939de9b1e791066a0224e331bc962a2099a7b1f04;
    uint256 internal constant COFFEE_KEY_Y = 0x64b8bbafe1535f2301c72c2cb3535b172da30b02686ab0393d348614f157fbdb;
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

    event ValidatorRegistered(
        address indexed validatorAddress, ValidatorManager.Secp256k1Key validatorKey, uint64 power
    );
    event ValidatorUnregistered(address indexed validatorAddress, ValidatorManager.Secp256k1Key validatorKey);
    event ValidatorPowerUpdated(
        address indexed validatorAddress, ValidatorManager.Secp256k1Key validatorKey, uint64 oldPower, uint64 newPower
    );

    function setUp() public {
        validatorManager = new ValidatorManager();
    }

    function validatorAddressFromKey(ValidatorManager.Secp256k1Key memory key) internal pure returns (address) {
        bytes32 hash = keccak256(abi.encodePacked(key.x, key.y));
        return address(uint160(uint256(hash)));
    }

    function uncompressedPublicKey(ValidatorManager.Secp256k1Key memory key) internal pure returns (bytes memory) {
        return abi.encodePacked(uint8(0x04), key.x, key.y);
    }

    function testOwnerCanRegisterValidator() public {
        bytes memory alicePublicKey = uncompressedPublicKey(aliceKey());
        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(validatorAddressFromKey(aliceKey()), aliceKey(), INITIAL_POWER);

        validatorManager.register(alicePublicKey, INITIAL_POWER);

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

    function testRegisterWithCompressedKey() public {
        bytes memory compressed = abi.encodePacked(uint8(0x03), DERIVED_PUBLIC_KEY_X);

        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(validatorAddressFromKey(mnemonicDerivedKey()), mnemonicDerivedKey(), INITIAL_POWER);

        validatorManager.register(compressed, INITIAL_POWER);

        ValidatorManager.ValidatorInfo memory info = validatorManager.getValidator(mnemonicDerivedKey());
        assertKeyEq(info.validatorKey, mnemonicDerivedKey());
        assertEq(info.power, INITIAL_POWER);
    }

    function testOwnerCanRegisterSetOfValidators() public {
        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(validatorAddressFromKey(aliceKey()), aliceKey(), INITIAL_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(validatorAddressFromKey(bobKey()), bobKey(), SECOND_POWER);

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
        bytes memory alicePublicKey = uncompressedPublicKey(aliceKey());
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.register(alicePublicKey, INITIAL_POWER);
    }

    function testRegisterRejectsInvalidInputs() public {
        bytes memory zeroPublicKey = uncompressedPublicKey(zeroKey());
        vm.expectRevert(ValidatorManager.InvalidKey.selector);
        validatorManager.register(zeroPublicKey, INITIAL_POWER);

        bytes memory alicePublicKey = uncompressedPublicKey(aliceKey());
        vm.expectRevert(ValidatorManager.InvalidPower.selector);
        validatorManager.register(alicePublicKey, 0);
    }

    function testRegisterRejectsDuplicateKey() public {
        bytes memory alicePublicKey = uncompressedPublicKey(aliceKey());
        validatorManager.register(alicePublicKey, INITIAL_POWER);

        vm.expectRevert(ValidatorManager.ValidatorAlreadyExists.selector);
        validatorManager.register(alicePublicKey, SECOND_POWER);
    }

    function testOwnerCanUpdatePower() public {
        bytes memory alicePublicKey = uncompressedPublicKey(aliceKey());
        validatorManager.register(alicePublicKey, INITIAL_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorPowerUpdated(validatorAddressFromKey(aliceKey()), aliceKey(), INITIAL_POWER, UPDATED_POWER);

        validatorManager.updatePower(aliceKey(), UPDATED_POWER);

        ValidatorManager.ValidatorInfo memory info = validatorManager.getValidator(aliceKey());
        assertEq(info.power, UPDATED_POWER);
        assertEq(validatorManager.getTotalPower(), UPDATED_POWER);
    }

    function testNonOwnerCannotUpdatePower() public {
        bytes memory alicePublicKey = uncompressedPublicKey(aliceKey());
        validatorManager.register(alicePublicKey, INITIAL_POWER);

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.updatePower(aliceKey(), UPDATED_POWER);
    }

    function testUpdatePowerRequiresExistingValidator() public {
        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.updatePower(aliceKey(), UPDATED_POWER);
    }

    function testOwnerCanUnregisterValidator() public {
        bytes memory alicePublicKey = uncompressedPublicKey(aliceKey());
        validatorManager.register(alicePublicKey, INITIAL_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorUnregistered(validatorAddressFromKey(aliceKey()), aliceKey());

        validatorManager.unregister(validatorAddressFromKey(aliceKey()));

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(aliceKey());
        assertEq(validatorManager.getValidatorCount(), 0);
        assertEq(validatorManager.getTotalPower(), 0);
        assertFalse(validatorManager.isValidator(aliceKey()));
    }

    function testOwnerCanUnregisterSetOfValidators() public {
        bytes memory alicePublicKey = uncompressedPublicKey(aliceKey());
        bytes memory bobPublicKey = uncompressedPublicKey(bobKey());
        validatorManager.register(alicePublicKey, INITIAL_POWER);
        validatorManager.register(bobPublicKey, SECOND_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorUnregistered(validatorAddressFromKey(aliceKey()), aliceKey());

        vm.expectEmit(true, false, false, true);
        emit ValidatorUnregistered(validatorAddressFromKey(bobKey()), bobKey());

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
        bytes memory alicePublicKey = uncompressedPublicKey(aliceKey());
        validatorManager.register(alicePublicKey, INITIAL_POWER);

        address aliceAddress = validatorAddressFromKey(aliceKey());
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.unregister(aliceAddress);
    }

    function testUnregisterRequiresExistingValidator() public {
        address aliceAddress = validatorAddressFromKey(aliceKey());
        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.unregister(aliceAddress);
    }

    function testGetValidatorRevertsWhenMissing() public {
        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(aliceKey());
    }

    function testGetValidatorsAggregatesAllEntries() public {
        bytes memory alicePublicKey = uncompressedPublicKey(aliceKey());
        bytes memory bobPublicKey = uncompressedPublicKey(bobKey());
        validatorManager.register(alicePublicKey, INITIAL_POWER);
        validatorManager.register(bobPublicKey, SECOND_POWER);

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

        bytes memory alicePublicKey = uncompressedPublicKey(aliceKey());
        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(validatorAddressFromKey(aliceKey()), aliceKey(), INITIAL_POWER);
        vm.prank(NEW_OWNER);
        validatorManager.register(alicePublicKey, INITIAL_POWER);

        bytes memory bobPublicKey = uncompressedPublicKey(bobKey());
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(this)));
        validatorManager.register(bobPublicKey, SECOND_POWER);
    }

    function testRenounceOwnershipLocksMutations() public {
        validatorManager.renounceOwnership();
        assertEq(validatorManager.owner(), address(0));

        bytes memory alicePublicKey = uncompressedPublicKey(aliceKey());
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(this)));
        validatorManager.register(alicePublicKey, INITIAL_POWER);

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.updatePower(aliceKey(), UPDATED_POWER);
    }

    function testOwnerCanAddAndRemoveValidators() public {
        bytes memory alicePublicKey = uncompressedPublicKey(aliceKey());
        bytes memory bobPublicKey = uncompressedPublicKey(bobKey());
        validatorManager.register(alicePublicKey, INITIAL_POWER);
        validatorManager.register(bobPublicKey, SECOND_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(validatorAddressFromKey(coffeeKey()), coffeeKey(), THIRD_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorUnregistered(validatorAddressFromKey(aliceKey()), aliceKey());

        vm.expectEmit(true, false, false, true);
        emit ValidatorUnregistered(validatorAddressFromKey(bobKey()), bobKey());

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

    function testValidatorAddressMatchesDerivedFromPrivateKey() public pure {
        address derived = validatorAddressFromKey(mnemonicDerivedKey());
        address expected = vm.addr(DERIVED_PRIVATE_KEY);
        assertEq(derived, expected);
    }
}
