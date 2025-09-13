use thiserror::Error;

/// Stratum client error types
#[derive(Error, Debug)]
pub enum StratumError {
    #[error("Connection error: {0}")]
    Connection(String),
    
    #[error("Protocol error: {0}")]
    Protocol(String),
    
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    
    #[error("Pool rejected share: {reason}")]
    ShareRejected { reason: String },
    
    #[error("Invalid job: {0}")]
    InvalidJob(String),
    
    #[error("JSON-RPC error: {code} - {message}")]
    JsonRpc { code: i32, message: String },
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("No pools available")]
    NoPoolsAvailable,
    
    #[error("All pools failed")]
    AllPoolsFailed,
    
    #[error("Timeout waiting for response")]
    Timeout,
    
    #[error("Client shutdown")]
    Shutdown,
    
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
    
    #[error("Unsupported algorithm: {0}")]
    UnsupportedAlgorithm(String),
}

pub type Result<T> = std::result::Result<T, StratumError>;