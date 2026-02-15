// SPDX-License-Identifier: Apache 2.0
pragma solidity ^0.8.28;

import {Test} from "forge-std/Test.sol";
import {ValidatorManager} from "../src/ValidatorManager.sol";
import {ValidatorManagerProxy} from "../src/ValidatorManagerProxy.sol";
import {ValidatorManagerV2} from "./ValidatorManagerV2.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {Initializable} from "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";

contract ValidatorManagerUpgradeTest is Test {
    ValidatorManager internal implementation;
    ValidatorManager internal validatorManager; // proxy cast
    ValidatorManagerProxy internal proxy;

    address internal constant NON_OWNER = address(0xBEEF);
    address internal constant NEW_OWNER = address(0xCAFE);

    bytes constant ALICE_UNCOMPRESSED =
        hex"048318535b54105d4a7aae60c08fc45f9687181b4fdfc625bd1a753fa7397fed753547f11ca8696646f2f3acb08e31016afac23e630c5d11f59f61fef57b0d2aa5";
    uint64 internal constant ALICE_POWER = 100;

    function setUp() public {
        implementation = new ValidatorManager();
        bytes memory initData = abi.encodeCall(ValidatorManager.initialize, (address(this)));
        proxy = new ValidatorManagerProxy(address(implementation), initData);
        validatorManager = ValidatorManager(address(proxy));
    }

    // -- Upgrade authorization ------------------------------------------------

    function testUpgradeByOwner() public {
        // Register a validator before upgrade
        validatorManager.register(ALICE_UNCOMPRESSED, ALICE_POWER);

        ValidatorManagerV2 v2Impl = new ValidatorManagerV2();
        validatorManager.upgradeToAndCall(address(v2Impl), "");

        // State survives
        assertEq(validatorManager.getValidatorCount(), 1);
        assertEq(validatorManager.getTotalPower(), ALICE_POWER);
        assertEq(validatorManager.owner(), address(this));

        // V2 function accessible
        assertEq(ValidatorManagerV2(address(proxy)).version(), 2);
    }

    function testUpgradeRevertsForNonOwner() public {
        ValidatorManagerV2 v2Impl = new ValidatorManagerV2();

        vm.expectRevert(
            abi.encodeWithSelector(OwnableUpgradeable.OwnableUnauthorizedAccount.selector, NON_OWNER)
        );
        vm.prank(NON_OWNER);
        validatorManager.upgradeToAndCall(address(v2Impl), "");
    }

    // -- State preservation ---------------------------------------------------

    function testStatePreservedAfterUpgrade() public {
        // Register two validators
        validatorManager.register(ALICE_UNCOMPRESSED, ALICE_POWER);

        ValidatorManager.Secp256k1Key memory aliceKey =
            validatorManager._secp256k1KeyFromBytes(ALICE_UNCOMPRESSED);
        address aliceAddr = validatorManager._validatorAddress(aliceKey);

        // Upgrade
        ValidatorManagerV2 v2Impl = new ValidatorManagerV2();
        validatorManager.upgradeToAndCall(address(v2Impl), "");

        // Verify all state
        assertEq(validatorManager.owner(), address(this));
        assertEq(validatorManager.getValidatorCount(), 1);
        assertEq(validatorManager.getTotalPower(), ALICE_POWER);
        assertTrue(validatorManager.isValidator(aliceAddr));

        ValidatorManager.ValidatorInfo memory info = validatorManager.getValidator(aliceAddr);
        assertKeyEq(info.validatorKey, aliceKey);
        assertEq(info.power, ALICE_POWER);
    }

    // -- Double initialization protection ------------------------------------

    function testCannotInitializeTwice() public {
        vm.expectRevert(Initializable.InvalidInitialization.selector);
        validatorManager.initialize(NON_OWNER);
    }

    function testCannotInitializeImplementation() public {
        vm.expectRevert(Initializable.InvalidInitialization.selector);
        implementation.initialize(NON_OWNER);
    }

    // -- Ownership transfer + upgrade ----------------------------------------

    function testTransferOwnershipThenUpgrade() public {
        validatorManager.transferOwnership(NEW_OWNER);
        assertEq(validatorManager.owner(), NEW_OWNER);

        // Old owner can no longer upgrade
        ValidatorManagerV2 v2Impl = new ValidatorManagerV2();
        vm.expectRevert(
            abi.encodeWithSelector(OwnableUpgradeable.OwnableUnauthorizedAccount.selector, address(this))
        );
        validatorManager.upgradeToAndCall(address(v2Impl), "");

        // New owner can upgrade
        vm.prank(NEW_OWNER);
        validatorManager.upgradeToAndCall(address(v2Impl), "");
        assertEq(ValidatorManagerV2(address(proxy)).version(), 2);
    }

    function testRenounceOwnershipBlocksUpgrade() public {
        validatorManager.renounceOwnership();
        assertEq(validatorManager.owner(), address(0));

        ValidatorManagerV2 v2Impl = new ValidatorManagerV2();
        vm.expectRevert(
            abi.encodeWithSelector(OwnableUpgradeable.OwnableUnauthorizedAccount.selector, address(this))
        );
        validatorManager.upgradeToAndCall(address(v2Impl), "");
    }

    // -- Helpers --------------------------------------------------------------

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
}
