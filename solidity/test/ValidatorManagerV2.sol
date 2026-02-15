// SPDX-License-Identifier: Apache 2.0
pragma solidity ^0.8.28;

import {ValidatorManager} from "../src/ValidatorManager.sol";

/**
 * @dev Minimal V2 mock used only for upgrade testing.
 *      Adds a `version()` view that returns 2.
 */
contract ValidatorManagerV2 is ValidatorManager {
    function version() external pure override returns (uint256) {
        return 2;
    }
}
