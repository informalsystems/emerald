// SPDX-License-Identifier: Apache 2.0
pragma solidity ^0.8.27;

import {Test} from "forge-std/Test.sol";
import {Counter} from "../src/Counter.sol";

contract CounterTest is Test {
    Counter public counter;

    function setUp() public {
        counter = new Counter();
        assertEq(counter.number(), 0);
    }

    function test_Increment() public {
        counter.increment();
        assertEq(counter.number(), 1);
    }

    function test_setNumber() public {
        counter.setNumber(42);
        assertEq(counter.number(), 42);
    }

    function testFuzz_SetNumber(uint256 x) public {
        counter.setNumber(x);
        assertEq(counter.number(), x);
    }
}
