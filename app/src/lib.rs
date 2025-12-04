//! Emerald application library
//!
//! This module exposes the internal application components for testing and integration.

pub mod app;
pub mod metrics;
pub mod node;
pub mod state;
pub mod store;
pub mod streaming;
pub mod sync_handler;

// Re-export commonly used types
pub use state::State;
pub use store::Store;
