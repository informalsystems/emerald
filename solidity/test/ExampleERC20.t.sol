// SPDX-License-Identifier: MIT
pragma solidity ^0.8.27;

import {Test} from "forge-std/Test.sol";
import {TestToken} from "../src/ExampleERC20.sol";

contract SimpleTest is Test {
    TestToken token;
    address owner = address(0x1);
    address user = address(0x2);

    function setUp() public {
        vm.prank(owner);
        token = new TestToken(owner);
    }

    function testMint() public {
        // Owner mints 100 tokens to user
        vm.prank(owner);
        token.mint(user, 100 * 10 ** 18);

        // Check user received tokens
        assertEq(token.balanceOf(user), 100 * 10 ** 18);
        assertEq(token.totalSupply(), 100 * 10 ** 18);
    }

    function testTransfer() public {
        // First mint tokens to user
        vm.prank(owner);
        token.mint(user, 100 * 10 ** 18);

        // User transfers 30 tokens to owner
        vm.prank(user);
        require(token.transfer(owner, 30 * 10 ** 18), "Transfer failed");

        // Check balances
        assertEq(token.balanceOf(user), 70 * 10 ** 18);
        assertEq(token.balanceOf(owner), 30 * 10 ** 18);
    }

    function testBurn() public {
        // Mint tokens to user
        vm.prank(owner);
        token.mint(user, 100 * 10 ** 18);

        // User burns 25 tokens
        vm.prank(user);
        token.burn(25 * 10 ** 18);

        // Check results
        assertEq(token.balanceOf(user), 75 * 10 ** 18);
        assertEq(token.totalSupply(), 75 * 10 ** 18);
    }
}
