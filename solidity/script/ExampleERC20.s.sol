// SPDX-License-Identifier: MIT
pragma solidity ^0.8.27;

import "forge-std/Script.sol";
import "../src/ExampleERC20.sol";

contract DeployTokenScript is Script {
    function run() external {
        vm.startBroadcast();

        // The deployer (msg.sender) will be the owner
        address owner = msg.sender;

        // Deploy the token
        TestToken token = new TestToken(owner);

        console.log("Token deployed at:", address(token));
        console.log("Owner set to:", owner);
        console.log("Token name:", token.name());
        console.log("Token symbol:", token.symbol());
        console.log("Initial supply:", token.totalSupply());

        vm.stopBroadcast();
    }
}
