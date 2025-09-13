//! MineOS Stratum Protocol Client
//! 
//! A production-ready Stratum V1 client implementation for cryptocurrency mining pools.
//! 
//! # Features
//! 
//! - Stratum V1 protocol support
//! - Automatic failover between multiple pools
//! - Connection resilience with automatic reconnection
//! - Share buffering during disconnections
//! - Multiple failover strategies (priority, round-robin, weighted, lowest-latency)
//! - Comprehensive error handling
//! - Async/await based on Tokio
//! 
//! # Example
//! 
//! ```no_run
//! use mineos_stratum::{StratumClient, StratumConfig, PoolConfig};
//! 
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Configure pools
//!     let config = StratumConfig {
//!         pools: vec![
//!             PoolConfig {
//!                 name: "primary".to_string(),
//!                 url: "stratum+tcp://pool.example.com:3333".to_string(),
//!                 username: "wallet.worker".to_string(),
//!                 password: "x".to_string(),
//!                 priority: 0,
//!                 weight: 1,
//!                 enabled: true,
//!             },
//!         ],
//!         ..Default::default()
//!     };
//!     
//!     // Create client
//!     let (client, mut job_rx) = StratumClient::new(config);
//!     
//!     // Start client
//!     client.start().await?;
//!     
//!     // Receive mining jobs
//!     while let Some(job) = job_rx.recv().await {
//!         println!("New job: {:?}", job);
//!         // Process job and submit shares...
//!     }
//!     
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod config;
pub mod connection;
pub mod error;
pub mod pool;
pub mod protocol;

// Re-export main types
pub use client::{ClientState, MiningStats, StratumClient};
pub use config::{FailoverStrategy, PoolConfig, StratumConfig};
pub use error::{Result, StratumError};
pub use protocol::{
    Difficulty, MiningJob, Share, StratumNotification, StratumRequest, StratumResponse,
    SubscribeResult, AuthorizeResult,
};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default user agent string
pub fn default_user_agent() -> String {
    format!("MineOS/{}", VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
    
    #[test]
    fn test_user_agent() {
        let ua = default_user_agent();
        assert!(ua.starts_with("MineOS/"));
    }
}