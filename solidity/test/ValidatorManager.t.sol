// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {Test} from "forge-std/Test.sol";
import {ValidatorManager} from "../src/ValidatorManager.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

contract ValidatorManagerTest is Test {
    ValidatorManager internal validatorManager;

    uint64 internal constant INITIAL_POWER = 100;
    uint64 internal constant UPDATED_POWER = 200;
    uint64 internal constant SECOND_POWER = 150;
    uint64 internal constant THIRD_POWER = 50;
    address internal constant NON_OWNER = address(0xBEEF);
    address internal constant NEW_OWNER = address(0xCAFE);

    uint256 internal constant ALICE_PRIVATE_KEY = 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80;

    bytes constant ALICE_UNCOMPRESSED =
        hex"048318535b54105d4a7aae60c08fc45f9687181b4fdfc625bd1a753fa7397fed753547f11ca8696646f2f3acb08e31016afac23e630c5d11f59f61fef57b0d2aa5";
    bytes constant BOB_COMPRESSED = hex"02ba5734d8f7091719471e7f7ed6b9df170dc70cc661ca05e688601ad984f068b0";
    bytes constant COFFEE_COMPRESSED = hex"039d9031e97dd78ff8c15aa86939de9b1e791066a0224e331bc962a2099a7b1f04";
    bytes constant ZERO_UNCOMPRESSED =
        hex"0400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

    address internal aliceValidatorAddress;
    address internal bobValidatorAddress;
    address internal coffeeValidatorAddress;

    ValidatorManager.Secp256k1Key internal aliceKey;
    ValidatorManager.Secp256k1Key internal bobKey;
    ValidatorManager.Secp256k1Key internal coffeeKey;

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
        ValidatorManager.Secp256k1Key memory aliceKeyMem = validatorManager._secp256k1KeyFromBytes(ALICE_UNCOMPRESSED);
        aliceKey = aliceKeyMem;
        aliceValidatorAddress = validatorManager._validatorAddress(aliceKeyMem);

        ValidatorManager.Secp256k1Key memory bobKeyMem = validatorManager._secp256k1KeyFromBytes(BOB_COMPRESSED);
        bobKey = bobKeyMem;
        bobValidatorAddress = validatorManager._validatorAddress(bobKeyMem);

        ValidatorManager.Secp256k1Key memory coffeeKeyMem = validatorManager._secp256k1KeyFromBytes(COFFEE_COMPRESSED);
        coffeeKey = coffeeKeyMem;
        coffeeValidatorAddress = validatorManager._validatorAddress(coffeeKeyMem);
    }

    function testOwnerCanRegisterValidator() public {
        bytes memory alicePublicKey = ALICE_UNCOMPRESSED;
        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(aliceValidatorAddress, aliceKey, INITIAL_POWER);

        validatorManager.register(alicePublicKey, INITIAL_POWER);

        ValidatorManager.ValidatorInfo memory info = validatorManager.getValidator(aliceValidatorAddress);
        assertKeyEq(info.validatorKey, aliceKey);
        assertEq(info.power, INITIAL_POWER);
        assertEq(validatorManager.getValidatorCount(), 1);
        assertEq(validatorManager.getTotalPower(), INITIAL_POWER);
        assertTrue(validatorManager.isValidator(aliceValidatorAddress));

        ValidatorManager.ValidatorInfo[] memory validators = validatorManager.getValidators();
        assertEq(validators.length, 1);
        assertKeyEq(validators[0].validatorKey, aliceKey);
    }

    function testRegisterWithUncompressedKey() public {
        bytes memory uncompressed = ALICE_UNCOMPRESSED;

        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(aliceValidatorAddress, aliceKey, INITIAL_POWER);

        validatorManager.register(uncompressed, INITIAL_POWER);

        ValidatorManager.ValidatorInfo memory info = validatorManager.getValidator(aliceValidatorAddress);
        assertKeyEq(info.validatorKey, aliceKey);
        assertEq(info.power, INITIAL_POWER);
    }

    function testOwnerCanRegisterSetOfValidators() public {
        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(aliceValidatorAddress, aliceKey, INITIAL_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(bobValidatorAddress, bobKey, SECOND_POWER);

        ValidatorManager.ValidatorRegistration[] memory registrations = new ValidatorManager.ValidatorRegistration[](2);
        registrations[0] = ValidatorManager.ValidatorRegistration({publicKey: ALICE_UNCOMPRESSED, power: INITIAL_POWER});
        registrations[1] = ValidatorManager.ValidatorRegistration({publicKey: BOB_COMPRESSED, power: SECOND_POWER});

        validatorManager.registerSet(registrations);

        assertEq(validatorManager.getValidatorCount(), 2);
        assertEq(validatorManager.getTotalPower(), INITIAL_POWER + SECOND_POWER);

        ValidatorManager.ValidatorInfo memory aliceInfo = validatorManager.getValidator(aliceValidatorAddress);
        assertKeyEq(aliceInfo.validatorKey, aliceKey);
        assertEq(aliceInfo.power, INITIAL_POWER);
        assertTrue(validatorManager.isValidator(aliceValidatorAddress));

        ValidatorManager.ValidatorInfo memory bobInfo = validatorManager.getValidator(bobValidatorAddress);
        assertKeyEq(bobInfo.validatorKey, bobKey);
        assertEq(bobInfo.power, SECOND_POWER);
        assertTrue(validatorManager.isValidator(bobValidatorAddress));

        ValidatorManager.ValidatorInfo[] memory validators = validatorManager.getValidators();
        assertEq(validators.length, 2);
        assertKeyEq(validators[0].validatorKey, aliceKey);
        assertKeyEq(validators[1].validatorKey, bobKey);
    }

    function testRegisterRevertsOnTotalPowerOverflow() public {
        validatorManager.register(ALICE_UNCOMPRESSED, type(uint64).max);

        vm.expectRevert(ValidatorManager.TotalPowerOverflow.selector);
        validatorManager.register(BOB_COMPRESSED, 1);
    }

    function testUpdatePowerRevertsOnTotalPowerOverflow() public {
        validatorManager.register(ALICE_UNCOMPRESSED, type(uint64).max - 1);
        validatorManager.register(BOB_COMPRESSED, 1);

        vm.expectRevert(ValidatorManager.TotalPowerOverflow.selector);
        validatorManager.updatePower(bobValidatorAddress, 2);
    }

    function testNonOwnerCannotRegisterValidator() public {
        bytes memory alicePublicKey = ALICE_UNCOMPRESSED;
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.register(alicePublicKey, INITIAL_POWER);
    }

    function testRegisterRejectsInvalidInputs() public {
        bytes memory zeroPublicKey = ZERO_UNCOMPRESSED;
        vm.expectRevert(ValidatorManager.InvalidKey.selector);
        validatorManager.register(zeroPublicKey, INITIAL_POWER);

        bytes memory alicePublicKey = ALICE_UNCOMPRESSED;
        vm.expectRevert(ValidatorManager.InvalidPower.selector);
        validatorManager.register(alicePublicKey, 0);
    }

    function testRegisterRejectsDuplicateKey() public {
        bytes memory alicePublicKey = ALICE_UNCOMPRESSED;
        validatorManager.register(alicePublicKey, INITIAL_POWER);

        vm.expectRevert(ValidatorManager.ValidatorAlreadyExists.selector);
        validatorManager.register(alicePublicKey, SECOND_POWER);
    }

    function testOwnerCanUpdatePower() public {
        bytes memory alicePublicKey = ALICE_UNCOMPRESSED;
        validatorManager.register(alicePublicKey, INITIAL_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorPowerUpdated(aliceValidatorAddress, aliceKey, INITIAL_POWER, UPDATED_POWER);

        validatorManager.updatePower(aliceValidatorAddress, UPDATED_POWER);

        ValidatorManager.ValidatorInfo memory info = validatorManager.getValidator(aliceValidatorAddress);
        assertEq(info.power, UPDATED_POWER);
        assertEq(validatorManager.getTotalPower(), UPDATED_POWER);
    }

    function testNonOwnerCannotUpdatePower() public {
        bytes memory alicePublicKey = ALICE_UNCOMPRESSED;
        validatorManager.register(alicePublicKey, INITIAL_POWER);

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.updatePower(aliceValidatorAddress, UPDATED_POWER);
    }

    function testUpdatePowerRequiresExistingValidator() public {
        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.updatePower(aliceValidatorAddress, UPDATED_POWER);
    }

    function testOwnerCanUnregisterValidator() public {
        bytes memory alicePublicKey = ALICE_UNCOMPRESSED;
        validatorManager.register(alicePublicKey, INITIAL_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorUnregistered(aliceValidatorAddress, aliceKey);

        validatorManager.unregister(aliceValidatorAddress);

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(aliceValidatorAddress);
        assertEq(validatorManager.getValidatorCount(), 0);
        assertEq(validatorManager.getTotalPower(), 0);
        assertFalse(validatorManager.isValidator(aliceValidatorAddress));
    }

    function testOwnerCanUnregisterSetOfValidators() public {
        bytes memory alicePublicKey = ALICE_UNCOMPRESSED;
        bytes memory bobPublicKey = BOB_COMPRESSED;
        validatorManager.register(alicePublicKey, INITIAL_POWER);
        validatorManager.register(bobPublicKey, SECOND_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorUnregistered(aliceValidatorAddress, aliceKey);

        vm.expectEmit(true, false, false, true);
        emit ValidatorUnregistered(bobValidatorAddress, bobKey);

        address[] memory addresses = new address[](2);
        addresses[0] = aliceValidatorAddress;
        addresses[1] = bobValidatorAddress;

        validatorManager.unregisterSet(addresses);

        assertEq(validatorManager.getValidatorCount(), 0);
        assertEq(validatorManager.getTotalPower(), 0);

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(aliceValidatorAddress);
        assertFalse(validatorManager.isValidator(aliceValidatorAddress));

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(bobValidatorAddress);
        assertFalse(validatorManager.isValidator(bobValidatorAddress));
    }

    function testNonOwnerCannotUnregisterValidator() public {
        bytes memory alicePublicKey = ALICE_UNCOMPRESSED;
        validatorManager.register(alicePublicKey, INITIAL_POWER);

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.unregister(aliceValidatorAddress);
    }

    function testUnregisterRequiresExistingValidator() public {
        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.unregister(aliceValidatorAddress);
    }

    function testGetValidatorRevertsWhenMissing() public {
        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(aliceValidatorAddress);
    }

    function testGetValidatorsAggregatesAllEntries() public {
        bytes memory alicePublicKey = ALICE_UNCOMPRESSED;
        bytes memory bobPublicKey = BOB_COMPRESSED;
        validatorManager.register(alicePublicKey, INITIAL_POWER);
        validatorManager.register(bobPublicKey, SECOND_POWER);

        ValidatorManager.ValidatorInfo[] memory validators = validatorManager.getValidators();
        assertEq(validators.length, 2);

        uint256 totalPower;
        bool sawAlice;
        bool sawBob;
        for (uint256 i = 0; i < validators.length; i++) {
            totalPower += validators[i].power;
            if (keysEqual(validators[i].validatorKey, aliceKey)) {
                assertEq(validators[i].power, INITIAL_POWER);
                sawAlice = true;
            } else if (keysEqual(validators[i].validatorKey, bobKey)) {
                assertEq(validators[i].power, SECOND_POWER);
                sawBob = true;
            }
        }

        assertTrue(sawAlice && sawBob);
        assertEq(totalPower, validatorManager.getTotalPower());
        assertEq(validatorManager.getValidatorCount(), 2);
    }

    function testGetValidatorAddressesReturnsAllRegistered() public {
        bytes memory alicePublicKey = ALICE_UNCOMPRESSED;
        bytes memory bobPublicKey = BOB_COMPRESSED;
        validatorManager.register(alicePublicKey, INITIAL_POWER);
        validatorManager.register(bobPublicKey, SECOND_POWER);

        address[] memory addresses = validatorManager.getValidatorAddresses();

        assertEq(addresses.length, 2);
        assertEq(addresses[0], aliceValidatorAddress);
        assertEq(addresses[1], bobValidatorAddress);
    }

    function testTransferOwnershipGivesControlToNewOwner() public {
        validatorManager.transferOwnership(NEW_OWNER);
        assertEq(validatorManager.owner(), NEW_OWNER);

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.transferOwnership(address(0xBAD));

        bytes memory alicePublicKey = ALICE_UNCOMPRESSED;
        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(aliceValidatorAddress, aliceKey, INITIAL_POWER);
        vm.prank(NEW_OWNER);
        validatorManager.register(alicePublicKey, INITIAL_POWER);

        bytes memory bobPublicKey = BOB_COMPRESSED;
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(this)));
        validatorManager.register(bobPublicKey, SECOND_POWER);
    }

    function testRenounceOwnershipLocksMutations() public {
        validatorManager.renounceOwnership();
        assertEq(validatorManager.owner(), address(0));

        bytes memory alicePublicKey = ALICE_UNCOMPRESSED;
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(this)));
        validatorManager.register(alicePublicKey, INITIAL_POWER);

        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, NON_OWNER));
        vm.prank(NON_OWNER);
        validatorManager.updatePower(aliceValidatorAddress, UPDATED_POWER);
    }

    function testOwnerCanAddAndRemoveValidators() public {
        bytes memory alicePublicKey = ALICE_UNCOMPRESSED;
        bytes memory bobPublicKey = BOB_COMPRESSED;
        validatorManager.register(alicePublicKey, INITIAL_POWER);
        validatorManager.register(bobPublicKey, SECOND_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorRegistered(coffeeValidatorAddress, coffeeKey, THIRD_POWER);

        vm.expectEmit(true, false, false, true);
        emit ValidatorUnregistered(aliceValidatorAddress, aliceKey);

        vm.expectEmit(true, false, false, true);
        emit ValidatorUnregistered(bobValidatorAddress, bobKey);

        ValidatorManager.ValidatorRegistration[] memory addRegistrations =
            new ValidatorManager.ValidatorRegistration[](1);
        addRegistrations[0] = ValidatorManager.ValidatorRegistration({publicKey: COFFEE_COMPRESSED, power: THIRD_POWER});
        address[] memory removeAddresses = new address[](2);
        removeAddresses[0] = aliceValidatorAddress;
        removeAddresses[1] = bobValidatorAddress;

        validatorManager.updateValidatorSet(addRegistrations, removeAddresses);

        assertEq(validatorManager.getValidatorCount(), 1);
        assertEq(validatorManager.getTotalPower(), THIRD_POWER);

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(aliceValidatorAddress);
        assertFalse(validatorManager.isValidator(aliceValidatorAddress));

        vm.expectRevert(ValidatorManager.ValidatorDoesNotExist.selector);
        validatorManager.getValidator(bobValidatorAddress);
        assertFalse(validatorManager.isValidator(bobValidatorAddress));

        ValidatorManager.ValidatorInfo memory info = validatorManager.getValidator(coffeeValidatorAddress);
        assertKeyEq(info.validatorKey, coffeeKey);
        assertEq(info.power, THIRD_POWER);
        assertTrue(validatorManager.isValidator(coffeeValidatorAddress));

        ValidatorManager.ValidatorInfo[] memory validators = validatorManager.getValidators();
        assertEq(validators.length, 1);
        assertKeyEq(validators[0].validatorKey, coffeeKey);
    }

    function testValidatorAddressMatchesDerivedFromPrivateKey() public view {
        address derived = vm.addr(ALICE_PRIVATE_KEY);
        assertEq(derived, aliceValidatorAddress);
    }
}
