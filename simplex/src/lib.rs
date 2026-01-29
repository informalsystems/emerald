//! Emerald Simplex - EVM execution with commonware simplex consensus.
//!
//! This crate provides a simplex consensus-based EVM execution layer that:
//! - Uses commonware's simplex consensus for agreement
//! - Reuses emerald's Engine API client for EVM execution
//! - Can be used as an alternative to malachite consensus in the emerald node

pub mod application;
pub mod block;
pub mod config;
pub mod consensus;
pub mod engine;
