//! xpm-core — Core library for the xpm package manager.
//!
//! This crate contains the business logic, configuration management,
//! and error types shared across the xpm ecosystem.

pub mod config;
pub mod error;

// Re-export key types for convenience.
pub use config::XpmConfig;
pub use error::{XpmError, XpmResult};
