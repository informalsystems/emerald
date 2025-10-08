//! Error types for storage data generation

use alloy_primitives::U256;
use thiserror::Error;

/// Result type for storage operations
pub type Result<T> = std::result::Result<T, StorageError>;

/// Errors that can occur during storage data generation
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Empty validator set")]
    EmptyValidatorSet,

    #[error("Invalid power for validator {0}")]
    InvalidPower(U256),

    #[error("Duplicate validator {0}")]
    DuplicateValidator(U256),
}
