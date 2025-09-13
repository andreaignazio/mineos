pub mod commands;
pub mod config;
pub mod dashboard;
pub mod utils;
pub mod client;
pub mod miner_service;

// Re-export commonly used types
pub use config::{MinerConfig, PoolConfig, GpuConfig};
pub use client::MinerClient;
pub use dashboard::Dashboard;