use alloy_primitives::{address, Address};

/// Genesis validator manager account address
pub const GENESIS_ACCOUNT: Address = address!("0x0000000000000000000000000000000000002000");

alloy_sol_types::sol!(
    #[derive(Debug)]
    #[sol(rpc)]
    ValidatorManager,
    "../solidity/out/ValidatorManager.sol/ValidatorManager.json"
);
