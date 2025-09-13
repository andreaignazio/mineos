use crate::{
    config::{FailoverStrategy, PoolConfig, StratumConfig},
    connection::StratumConnection,
    error::{Result, StratumError},
    protocol::{StratumNotification, StratumRequest, StratumResponse},
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Pool connection metrics
#[derive(Debug, Clone)]
struct PoolMetrics {
    pub latency: Duration,
    pub last_success: Option<Instant>,
    pub failure_count: u32,
    pub success_count: u32,
}

/// Connection pool for managing multiple pool connections
pub struct ConnectionPool {
    /// Configuration
    config: StratumConfig,
    
    /// Pool connections
    connections: Arc<RwLock<HashMap<String, Arc<StratumConnection>>>>,
    
    /// Current active pool
    active_pool: Arc<RwLock<Option<String>>>,
    
    /// Pool metrics
    metrics: Arc<RwLock<HashMap<String, PoolMetrics>>>,
    
    /// Notification channel (shared across all connections)
    notification_tx: mpsc::Sender<StratumNotification>,
}

impl ConnectionPool {
    /// Create new connection pool
    pub fn new(config: StratumConfig, notification_tx: mpsc::Sender<StratumNotification>) -> Self {
        Self {
            config,
            connections: Arc::new(RwLock::new(HashMap::new())),
            active_pool: Arc::new(RwLock::new(None)),
            metrics: Arc::new(RwLock::new(HashMap::new())),
            notification_tx,
        }
    }
    
    /// Connect to pools based on strategy
    pub async fn connect(&self) -> Result<()> {
        if self.config.pools.is_empty() {
            return Err(StratumError::NoPoolsAvailable);
        }
        
        // Initialize connections for all enabled pools
        for pool_config in &self.config.pools {
            if !pool_config.enabled {
                continue;
            }
            
            let (host, port) = pool_config.parse_url()
                .map_err(|e| StratumError::InvalidConfiguration(e))?;
            
            let connection = Arc::new(StratumConnection::new(
                host,
                port,
                self.notification_tx.clone(),
                self.config.connection_timeout,
                self.config.response_timeout,
                self.config.max_reconnect_attempts,
                self.config.reconnect_backoff_ms,
                self.config.max_reconnect_backoff_ms,
            ));
            
            self.connections.write().await.insert(
                pool_config.name.clone(),
                connection,
            );
            
            self.metrics.write().await.insert(
                pool_config.name.clone(),
                PoolMetrics {
                    latency: Duration::from_secs(0),
                    last_success: None,
                    failure_count: 0,
                    success_count: 0,
                },
            );
        }
        
        // Try to connect to primary pool
        self.connect_to_best_pool().await
    }
    
    /// Connect to the best available pool based on strategy
    async fn connect_to_best_pool(&self) -> Result<()> {
        let pool_name = match self.config.failover_strategy {
            FailoverStrategy::Priority => self.get_priority_pool().await,
            FailoverStrategy::RoundRobin => self.get_round_robin_pool().await,
            FailoverStrategy::Weighted => self.get_weighted_pool().await,
            FailoverStrategy::LowestLatency => self.get_lowest_latency_pool().await,
        }?;
        
        match self.connect_to_pool(&pool_name).await {
            Ok(()) => Ok(()),
            Err(_) => self.failover().await,
        }
    }
    
    /// Connect to specific pool
    async fn connect_to_pool(&self, pool_name: &str) -> Result<()> {
        info!("Connecting to pool: {}", pool_name);
        
        let connections = self.connections.read().await;
        let connection = connections.get(pool_name)
            .ok_or(StratumError::InvalidConfiguration(
                format!("Pool {} not found", pool_name)
            ))?;
        
        match connection.connect().await {
            Ok(()) => {
                *self.active_pool.write().await = Some(pool_name.to_string());
                
                // Update metrics
                if let Some(metrics) = self.metrics.write().await.get_mut(pool_name) {
                    metrics.last_success = Some(Instant::now());
                    metrics.success_count += 1;
                }
                
                info!("Successfully connected to pool: {}", pool_name);
                Ok(())
            }
            Err(e) => {
                error!("Failed to connect to pool {}: {}", pool_name, e);
                
                // Update metrics
                if let Some(metrics) = self.metrics.write().await.get_mut(pool_name) {
                    metrics.failure_count += 1;
                }
                
                // Return error, let caller decide on failover
                Err(e)
            }
        }
    }
    
    /// Failover to next available pool
    async fn failover(&self) -> Result<()> {
        warn!("Attempting failover to backup pool");
        
        let current_pool = self.active_pool.read().await.clone();
        let mut tried_pools = vec![current_pool.clone()];
        
        // Try each pool in order of strategy
        for _ in 0..self.config.pools.len() {
            let next_pool = match self.config.failover_strategy {
                FailoverStrategy::Priority => self.get_priority_pool_excluding(&tried_pools).await,
                FailoverStrategy::RoundRobin => self.get_next_pool_after(&current_pool).await,
                FailoverStrategy::Weighted => self.get_weighted_pool_excluding(&tried_pools).await,
                FailoverStrategy::LowestLatency => self.get_lowest_latency_pool_excluding(&tried_pools).await,
            };
            
            if let Ok(pool_name) = next_pool {
                if self.connect_to_pool(&pool_name).await.is_ok() {
                    return Ok(());
                }
                tried_pools.push(Some(pool_name));
            } else {
                break;
            }
        }
        
        Err(StratumError::AllPoolsFailed)
    }
    
    /// Send request to active pool
    pub async fn send_request(&self, request: StratumRequest) -> Result<StratumResponse> {
        let active = self.active_pool.read().await.clone()
            .ok_or(StratumError::NoPoolsAvailable)?;
        
        let connections = self.connections.read().await;
        let connection = connections.get(&active)
            .ok_or(StratumError::NoPoolsAvailable)?;
        
        let start = Instant::now();
        
        match connection.send_request(request.clone()).await {
            Ok(response) => {
                // Update latency metric
                if let Some(metrics) = self.metrics.write().await.get_mut(&active) {
                    metrics.latency = start.elapsed();
                    metrics.last_success = Some(Instant::now());
                }
                Ok(response)
            }
            Err(e) => {
                warn!("Request failed on pool {}: {}", active, e);
                
                // Update failure metric
                if let Some(metrics) = self.metrics.write().await.get_mut(&active) {
                    metrics.failure_count += 1;
                }
                
                // Return error and let client handle failover
                Err(e)
            }
        }
    }
    
    /// Disconnect from all pools
    pub async fn disconnect(&self) {
        info!("Disconnecting from all pools");
        
        for connection in self.connections.read().await.values() {
            connection.disconnect().await;
        }
        
        *self.active_pool.write().await = None;
    }
    
    /// Check if any pool is connected
    pub async fn is_connected(&self) -> bool {
        if let Some(active) = self.active_pool.read().await.as_ref() {
            if let Some(connection) = self.connections.read().await.get(active) {
                return connection.is_connected().await;
            }
        }
        false
    }
    
    /// Set authenticated state for active pool
    pub async fn set_authenticated(&self, authenticated: bool) {
        if let Some(active) = self.active_pool.read().await.as_ref() {
            if let Some(connection) = self.connections.read().await.get(active) {
                connection.set_authenticated(authenticated).await;
            }
        }
    }
    
    /// Get current active pool configuration
    pub async fn current_pool(&self) -> Option<PoolConfig> {
        let active = self.active_pool.read().await.clone()?;
        self.config.pools.iter()
            .find(|p| p.name == active)
            .cloned()
    }
    
    // Strategy implementations
    
    async fn get_priority_pool(&self) -> Result<String> {
        self.config.pools.iter()
            .filter(|p| p.enabled)
            .min_by_key(|p| p.priority)
            .map(|p| p.name.clone())
            .ok_or(StratumError::NoPoolsAvailable)
    }
    
    async fn get_priority_pool_excluding(&self, exclude: &[Option<String>]) -> Result<String> {
        self.config.pools.iter()
            .filter(|p| p.enabled && !exclude.contains(&Some(p.name.clone())))
            .min_by_key(|p| p.priority)
            .map(|p| p.name.clone())
            .ok_or(StratumError::NoPoolsAvailable)
    }
    
    async fn get_round_robin_pool(&self) -> Result<String> {
        // Simple round-robin: just get the first enabled pool
        self.config.pools.iter()
            .find(|p| p.enabled)
            .map(|p| p.name.clone())
            .ok_or(StratumError::NoPoolsAvailable)
    }
    
    async fn get_next_pool_after(&self, current: &Option<String>) -> Result<String> {
        let pools: Vec<_> = self.config.pools.iter()
            .filter(|p| p.enabled)
            .collect();
        
        if pools.is_empty() {
            return Err(StratumError::NoPoolsAvailable);
        }
        
        if let Some(current_name) = current {
            if let Some(current_idx) = pools.iter().position(|p| p.name == *current_name) {
                let next_idx = (current_idx + 1) % pools.len();
                return Ok(pools[next_idx].name.clone());
            }
        }
        
        Ok(pools[0].name.clone())
    }
    
    async fn get_weighted_pool(&self) -> Result<String> {
        use rand::Rng;
        
        let pools: Vec<_> = self.config.pools.iter()
            .filter(|p| p.enabled)
            .collect();
        
        if pools.is_empty() {
            return Err(StratumError::NoPoolsAvailable);
        }
        
        let total_weight: u32 = pools.iter().map(|p| p.weight).sum();
        let mut rng = rand::thread_rng();
        let mut random = rng.gen_range(0..total_weight);
        
        for pool in &pools {
            if random < pool.weight {
                return Ok(pool.name.clone());
            }
            random -= pool.weight;
        }
        
        Ok(pools[0].name.clone())
    }
    
    async fn get_weighted_pool_excluding(&self, exclude: &[Option<String>]) -> Result<String> {
        use rand::Rng;
        
        let pools: Vec<_> = self.config.pools.iter()
            .filter(|p| p.enabled && !exclude.contains(&Some(p.name.clone())))
            .collect();
        
        if pools.is_empty() {
            return Err(StratumError::NoPoolsAvailable);
        }
        
        let total_weight: u32 = pools.iter().map(|p| p.weight).sum();
        let mut rng = rand::thread_rng();
        let mut random = rng.gen_range(0..total_weight);
        
        for pool in &pools {
            if random < pool.weight {
                return Ok(pool.name.clone());
            }
            random -= pool.weight;
        }
        
        Ok(pools[0].name.clone())
    }
    
    async fn get_lowest_latency_pool(&self) -> Result<String> {
        let metrics = self.metrics.read().await;
        
        self.config.pools.iter()
            .filter(|p| p.enabled)
            .min_by_key(|p| {
                metrics.get(&p.name)
                    .map(|m| m.latency)
                    .unwrap_or(Duration::from_secs(u64::MAX))
            })
            .map(|p| p.name.clone())
            .ok_or(StratumError::NoPoolsAvailable)
    }
    
    async fn get_lowest_latency_pool_excluding(&self, exclude: &[Option<String>]) -> Result<String> {
        let metrics = self.metrics.read().await;
        
        self.config.pools.iter()
            .filter(|p| p.enabled && !exclude.contains(&Some(p.name.clone())))
            .min_by_key(|p| {
                metrics.get(&p.name)
                    .map(|m| m.latency)
                    .unwrap_or(Duration::from_secs(u64::MAX))
            })
            .map(|p| p.name.clone())
            .ok_or(StratumError::NoPoolsAvailable)
    }
}

// Helper for random selection
mod rand {
    pub use ::rand::*;
}