//! # ValidatorSet Storage Generator
//!
//! This library provides functionality to generate storage slots and values
//! for the ValidatorSet smart contract based on a given validator list.

pub mod error;
#[cfg(test)]
mod tests;
pub mod types;

pub use emerald_contracts::{
    ValidatorManager, ValidatorManagerProxy, GENESIS_VALIDATOR_MANAGER_ACCOUNT,
    GENESIS_VALIDATOR_MANAGER_IMPL_ACCOUNT,
};
pub use error::{Error as ValidatorManagerError, Result};
pub use types::{Validator, ValidatorKey, ValidatorSet};
