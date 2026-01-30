//! Custom error messages for CLI helper functions.
//! This low level implementation allows the developer to choose their own error handling library.
use std::path::PathBuf;

/// Error messages for commands
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Error creating parent directory
    #[error("Error creating parent directory: {}", .0.display())]
    ParentDir(PathBuf),

    /// Error opening file
    #[error("Error opening file: {}", .0.display())]
    OpenFile(PathBuf),

    /// Error writing file
    #[error("Error writing file: {}", .0.display())]
    WriteFile(PathBuf),

    /// Error loading file
    #[error("Error loading file: {}. Details: {}", .0.display(), .1)]
    LoadFile(PathBuf, std::io::Error),

    /// Error converting to JSON
    #[error("Error converting to JSON: {0}")]
    ToJSON(String),

    /// Error determining home directory path
    #[error("Error determining home directory path")]
    DirPath,

    /// Error joining threads
    #[error("Error joining threads")]
    Join,

    #[error("Error decoding .toml file. Details: {0}")]
    FromTOML(toml::de::Error),

    #[error("Missing configuration path for node {0}")]
    MissingPath(usize),

    #[error("Missing moniker for node {0}")]
    MissingMoniker(usize),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}
