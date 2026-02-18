// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.29;

import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";

// This file exists to ensure Foundry emits an `ERC1967Proxy` artifact under `solidity/out`.
// Rust bindings consume that artifact directly.
