use std::fmt;
use std::path::PathBuf;

/// Errors that can occur during configuration parsing and validation.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration file not found: {path}")]
    NotFound { path: PathBuf },

    #[error("failed to read configuration file: {source}")]
    ReadError {
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse configuration: {source}")]
    ParseError {
        #[source]
        source: toml::de::Error,
    },

    #[error("invalid configuration: {message}")]
    Validation { message: String },
}

/// Top-level error type for all xpm operations.
#[derive(Debug, thiserror::Error)]
pub enum XpmError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("package not found: {name}")]
    PackageNotFound { name: String },

    #[error("dependency conflict: {0}")]
    DependencyConflict(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("transaction failed: {0}")]
    Transaction(String),

    #[error("signature verification failed: {0}")]
    SignatureError(String),

    #[error("package error: {0}")]
    Package(String),

    #[error("{0}")]
    Other(String),
}

/// Convenience type alias for xpm results.
pub type XpmResult<T> = Result<T, XpmError>;

impl fmt::Display for crate::config::SigLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            crate::config::SigLevel::Required => write!(f, "Required"),
            crate::config::SigLevel::Optional => write!(f, "Optional"),
            crate::config::SigLevel::Never => write!(f, "Never"),
        }
    }
}
