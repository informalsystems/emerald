// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Script, console} from "forge-std/Script.sol";
import {ValidatorSet} from "../src/ValidatorSet.sol";

contract ValidatorSetScript is Script {
    function run() external {
        // --- Setup ---
        // Get the deployer's private key from the environment
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(deployerPrivateKey);

        // --- 1. Deploy the contract ---
        ValidatorSet validatorSet = new ValidatorSet();
        console.log("ValidatorSet contract deployed at:", address(validatorSet));
        vm.stopBroadcast(); // Stop broadcasting as deployer, will use prank for subsequent calls

        // --- 2. Register multiple validators in non-sorted order ---
        // Setup validator accounts and data
        address validator1 = address(0x1000);
        address validator2 = address(0x3000);
        address validator3 = address(0x2000);

        bytes32 VAL_PK_1 = keccak256("val_pk_1");
        bytes32 VAL_PK_2 = keccak256("val_pk_2");
        bytes32 VAL_PK_3 = keccak256("val_pk_3");

        console.log("\nRegistering 3 validators in non-sequential order...");

        vm.prank(validator2);
        validatorSet.register(VAL_PK_2, 100);
        console.log("- Registered:", validator2);

        vm.prank(validator1);
        validatorSet.register(VAL_PK_1, 150);
        console.log("- Registered:", validator1);

        vm.prank(validator3);
        validatorSet.register(VAL_PK_3, 200);
        console.log("- Registered:", validator3);

        // --- 3. Verify the sorted list ---
        console.log("\n--- Verifying sorted validator set ---");
        logValidators(validatorSet);

        // --- 4. Unregister the middle validator ---
        console.log("\nUnregistering the middle validator:", validator3);
        vm.prank(validator3);
        validatorSet.unregister();

        // --- 5. Verify the list remains sorted ---
        console.log("\n--- Verifying sorted set after removing middle element ---");
        logValidators(validatorSet);

        // --- 6. Update voting power for one of the remaining validators ---
        uint64 newPower = 500;
        console.log("\nUpdating voting power for", validator1, "to", newPower);
        vm.prank(validator1);
        validatorSet.updateVotingPower(newPower);

        console.log("\n--- Verifying validator details after update ---");
        ValidatorSet.ValidatorDetails memory updatedValidator = validatorSet.getValidator(validator1);
        console.log(" -> ETH Address:", updatedValidator.ethAddress);
        console.log(" -> Public Key:");
        console.logBytes32(updatedValidator.ed25519PublicKey);
        console.log(" -> Voting Power:", updatedValidator.votingPower);

        // --- 7. Clean up: Unregister remaining validators ---
        console.log("\nCleaning up...");
        vm.prank(validator1);
        validatorSet.unregister();
        console.log("- Unregistered:", validator1);
        vm.prank(validator2);
        validatorSet.unregister();
        console.log("- Unregistered:", validator2);

        console.log("\n--- Final validator set (should be empty) ---");
        logValidators(validatorSet);
    }

    function logValidators(ValidatorSet validatorSet) internal view {
        ValidatorSet.ValidatorDetails[] memory allValidators = validatorSet.getValidators();
        console.log("Found", allValidators.length, "validator(s).");
        for (uint256 i = 0; i < allValidators.length; i++) {
            console.log("  Validator #%s:", i + 1);
            console.log("    ETH Address: ", allValidators[i].ethAddress);
            console.log("    Public Key:  ");
            console.logBytes32(allValidators[i].ed25519PublicKey);
            console.log("    Voting Power:", allValidators[i].votingPower);
        }
    }
}

contract GenesisScript is Script {
    // A helper struct to define initial validators in Solidity.
    struct InitialValidator {
        address ethAddress;
        bytes32 ed25519PublicKey;
        uint64 votingPower;
    }

    function run() external {
        // Define the initial set of validators.
        InitialValidator[] memory initialValidators = new InitialValidator[](3);
        initialValidators[0] = InitialValidator({
            ethAddress: 0x70997970C51812dc3A010C7d01b50e0d17dc79C8, // Anvil #2
            ed25519PublicKey: 0x1000000000000000000000000000000000000000000000000000000000000000,
            votingPower: 100
        });
        initialValidators[1] = InitialValidator({
            ethAddress: 0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC, // Anvil #3
            ed25519PublicKey: 0x2000000000000000000000000000000000000000000000000000000000000000,
            votingPower: 150
        });
        initialValidators[2] = InitialValidator({
            ethAddress: 0x90F79bf6EB2c4f870365E785982E1f101E93b906, // Anvil #4
            ed25519PublicKey: 0x3000000000000000000000000000000000000000000000000000000000000000,
            votingPower: 120
        });

        // The private key of the deployer (Anvil #0 account)
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");

        // --- Step 1: Deploy the contract using a broadcast ---
        vm.startBroadcast(deployerPrivateKey);
        ValidatorSet validatorSet = new ValidatorSet();
        vm.stopBroadcast(); // <-- Stop the broadcast immediately after deployment

        console.log("ValidatorSet deployed to:", address(validatorSet));

        // --- Step 2: Register validators using pranks ---
        // We loop through each validator and impersonate them for the register call.
        for (uint256 i = 0; i < initialValidators.length; i++) {
            InitialValidator memory v = initialValidators[i];

            // `vm.prank` sets msg.sender for the *next single external call*
            vm.prank(v.ethAddress);
            validatorSet.register(v.ed25519PublicKey, v.votingPower);

            console.log("Registered validator:", v.ethAddress);
        }
    }
}
