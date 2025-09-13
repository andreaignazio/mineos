use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Pool configuration for Stratum connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Pool name for identification
    pub name: String,
    
    /// Pool URL (hostname:port or stratum+tcp://hostname:port)
    pub url: String,
    
    /// Worker username (usually wallet.worker_name)
    pub username: String,
    
    /// Worker password (often just 'x' for most pools)
    pub password: String,
    
    /// Priority (lower = higher priority)
    #[serde(default = "default_priority")]
    pub priority: u32,
    
    /// Weight for load balancing (if using weighted strategy)
    #[serde(default = "default_weight")]
    pub weight: u32,
    
    /// Enable this pool
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Stratum client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StratumConfig {
    /// List of mining pools
    pub pools: Vec<PoolConfig>,
    
    /// Failover strategy
    #[serde(default)]
    pub failover_strategy: FailoverStrategy,
    
    /// Connection timeout
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: Duration,
    
    /// Keepalive interval
    #[serde(default = "default_keepalive_interval")]
    pub keepalive_interval: Duration,
    
    /// Response timeout
    #[serde(default = "default_response_timeout")]
    pub response_timeout: Duration,
    
    /// Maximum reconnection attempts
    #[serde(default = "default_max_reconnect_attempts")]
    pub max_reconnect_attempts: u32,
    
    /// Reconnection backoff base (milliseconds)
    #[serde(default = "default_reconnect_backoff_ms")]
    pub reconnect_backoff_ms: u64,
    
    /// Maximum reconnection backoff (milliseconds)
    #[serde(default = "default_max_reconnect_backoff_ms")]
    pub max_reconnect_backoff_ms: u64,
    
    /// Share submission retry attempts
    #[serde(default = "default_share_retry_attempts")]
    pub share_retry_attempts: u32,
    
    /// Buffer size for pending shares during disconnect
    #[serde(default = "default_share_buffer_size")]
    pub share_buffer_size: usize,
    
    /// Extra nonce size (for large farms)
    #[serde(default)]
    pub extra_nonce_size: Option<u32>,
    
    /// User agent string
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
}

/// Failover strategy for pool switching
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FailoverStrategy {
    /// Use pools in priority order
    #[default]
    Priority,
    
    /// Round-robin between pools
    RoundRobin,
    
    /// Weighted distribution
    Weighted,
    
    /// Lowest latency pool
    LowestLatency,
}

impl Default for StratumConfig {
    fn default() -> Self {
        Self {
            pools: Vec::new(),
            failover_strategy: FailoverStrategy::default(),
            connection_timeout: default_connection_timeout(),
            keepalive_interval: default_keepalive_interval(),
            response_timeout: default_response_timeout(),
            max_reconnect_attempts: default_max_reconnect_attempts(),
            reconnect_backoff_ms: default_reconnect_backoff_ms(),
            max_reconnect_backoff_ms: default_max_reconnect_backoff_ms(),
            share_retry_attempts: default_share_retry_attempts(),
            share_buffer_size: default_share_buffer_size(),
            extra_nonce_size: None,
            user_agent: default_user_agent(),
        }
    }
}

impl PoolConfig {
    /// Parse URL to extract host and port
    pub fn parse_url(&self) -> Result<(String, u16), String> {
        let url = self.url
            .strip_prefix("stratum+tcp://")
            .or_else(|| self.url.strip_prefix("stratum://"))
            .unwrap_or(&self.url);
        
        let parts: Vec<&str> = url.split(':').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid pool URL format: {}", self.url));
        }
        
        let host = parts[0].to_string();
        let port = parts[1].parse::<u16>()
            .map_err(|_| format!("Invalid port in URL: {}", self.url))?;
        
        Ok((host, port))
    }
}

// Default value functions for serde
fn default_priority() -> u32 { 0 }
fn default_weight() -> u32 { 1 }
fn default_enabled() -> bool { true }
fn default_connection_timeout() -> Duration { Duration::from_secs(30) }
fn default_keepalive_interval() -> Duration { Duration::from_secs(60) }
fn default_response_timeout() -> Duration { Duration::from_secs(10) }
fn default_max_reconnect_attempts() -> u32 { 10 }
fn default_reconnect_backoff_ms() -> u64 { 1000 }
fn default_max_reconnect_backoff_ms() -> u64 { 60000 }
fn default_share_retry_attempts() -> u32 { 3 }
fn default_share_buffer_size() -> usize { 100 }
fn default_user_agent() -> String { 
    format!("MineOS/{}", env!("CARGO_PKG_VERSION"))
}