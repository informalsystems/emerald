use alloy_primitives::{address, Address};

pub const GENESIS_VALIDATOR_SET_ACCOUNT: Address =
    address!("0000000000000000000000000000000000002000");

alloy_sol_types::sol!(
    #[derive(Debug)]
    #[sol(rpc)]
    ValidatorSet,
    "../solidity/out/ValidatorSet.sol/ValidatorSet.json",
);
