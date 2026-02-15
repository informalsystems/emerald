// SPDX-License-Identifier: Apache 2.0
pragma solidity ^0.8.28;

import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";

/**
 * @title ValidatorManagerProxy
 * @dev Thin ERC1967 proxy wrapper used for the ValidatorManager UUPS deployment.
 *      Exists as a concrete contract so that Foundry emits a build artifact that
 *      Rust can reference via the `sol!` macro.
 */
contract ValidatorManagerProxy is ERC1967Proxy {
    constructor(address logic, bytes memory data) ERC1967Proxy(logic, data) {}
}
