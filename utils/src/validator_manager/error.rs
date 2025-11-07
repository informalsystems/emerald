//! Error types for storage data generation

use alloy_primitives::U256;
use thiserror::Error;

/// Result type for storage operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during storage data generation
#[derive(Debug, Error)]
pub enum Error {
    #[error("Empty validator set")]
    EmptyValidatorSet,

    #[error("Invalid power for validator ({x:#x}, {y:#x})")]
    InvalidPower { x: U256, y: U256 },

    #[error("Duplicate validator ({x:#x}, {y:#x})")]
    DuplicateValidator { x: U256, y: U256 },

    #[error("Total validator power exceeds uint64 max")]
    TotalPowerOverflow,
}
