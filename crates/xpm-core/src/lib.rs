//! xpm-core — Core library for the xpm package manager.
//!
//! This crate contains the business logic, configuration management,
//! and error types shared across the xpm ecosystem.

pub mod config;
pub mod error;
pub mod hooks;
pub mod package;
pub mod repo;
pub mod repo_db;
pub mod repo_sync;
pub mod resolver;
pub mod transaction;

// Re-export key types for convenience.
pub use config::XpmConfig;
pub use error::{XpmError, XpmResult};
pub use transaction::{Transaction, TransactionOp, TransactionState, FileLock};
pub use hooks::{Hook, HookChain, HookContext, OperationType};
