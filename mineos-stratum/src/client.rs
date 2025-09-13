use crate::{
    config::{PoolConfig, StratumConfig},
    connection::{ConnectionState, StratumConnection},
    error::{Result, StratumError},
    pool::ConnectionPool,
    protocol::{
        methods, Difficulty, MiningJob, Share, StratumNotification,
        StratumRequest, StratumResponse, SubscribeResult,
    },
};
use arc_swap::ArcSwap;
use serde_json::Value;
use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{mpsc, RwLock, Mutex},
    time::{interval, sleep},
};
use tracing::{debug, error, info, warn};

/// Stratum client state
#[derive(Debug, Clone)]
pub struct ClientState {
    /// Current mining job
    pub current_job: Option<MiningJob>,
    
    /// Current difficulty
    pub difficulty: Difficulty,
    
    /// Extra nonce 1 from pool
    pub extra_nonce1: Option<String>,
    
    /// Extra nonce 2 size
    pub extra_nonce2_size: usize,
    
    /// Session ID for reconnection
    pub session_id: Option<String>,
    
    /// Is authorized
    pub authorized: bool,
    
    /// Statistics
    pub stats: MiningStats,
}

/// Mining statistics
#[derive(Debug, Clone, Default)]
pub struct MiningStats {
    pub shares_accepted: u64,
    pub shares_rejected: u64,
    pub shares_stale: u64,
    pub last_share_time: Option<Instant>,
    pub connection_start: Option<Instant>,
    pub total_uptime: Duration,
}

/// Main Stratum client
pub struct StratumClient {
    /// Configuration
    config: Arc<StratumConfig>,
    
    /// Connection pool
    pool: Arc<ConnectionPool>,
    
    /// Client state
    state: Arc<RwLock<ClientState>>,
    
    /// Notification receiver
    notification_rx: Arc<Mutex<mpsc::Receiver<StratumNotification>>>,
    
    /// Job update channel
    job_tx: mpsc::Sender<MiningJob>,
    
    /// Share submission queue (for buffering during disconnect)
    share_queue: Arc<Mutex<VecDeque<Share>>>,
    
    /// Shutdown signal
    shutdown_tx: mpsc::Sender<()>,
    shutdown_rx: Arc<Mutex<mpsc::Receiver<()>>>,
}

impl StratumClient {
    /// Create new Stratum client
    pub fn new(config: StratumConfig) -> (Self, mpsc::Receiver<MiningJob>) {
        let (notification_tx, notification_rx) = mpsc::channel(100);
        let (job_tx, job_rx) = mpsc::channel(10);
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        
        let pool = Arc::new(ConnectionPool::new(
            config.clone(),
            notification_tx,
        ));
        
        let state = Arc::new(RwLock::new(ClientState {
            current_job: None,
            difficulty: Difficulty(1.0),
            extra_nonce1: None,
            extra_nonce2_size: 4,
            session_id: None,
            authorized: false,
            stats: MiningStats::default(),
        }));
        
        let client = Self {
            config: Arc::new(config),
            pool,
            state,
            notification_rx: Arc::new(Mutex::new(notification_rx)),
            job_tx,
            share_queue: Arc::new(Mutex::new(VecDeque::with_capacity(100))),
            shutdown_tx,
            shutdown_rx: Arc::new(Mutex::new(shutdown_rx)),
        };
        
        (client, job_rx)
    }
    
    /// Start the client
    pub async fn start(&self) -> Result<()> {
        info!("Starting Stratum client");
        
        // Connect to pool
        self.pool.connect().await?;
        
        // Start notification handler
        self.start_notification_handler().await;
        
        // Start share submission handler
        self.start_share_handler().await;
        
        // Start keepalive task
        self.start_keepalive().await;
        
        // Subscribe to pool
        self.subscribe().await?;
        
        // Authorize worker
        let pool_config = self.pool.current_pool().await
            .ok_or(StratumError::NoPoolsAvailable)?;
        self.authorize(&pool_config.username, &pool_config.password).await?;
        
        Ok(())
    }
    
    /// Stop the client
    pub async fn stop(&self) {
        info!("Stopping Stratum client");
        let _ = self.shutdown_tx.send(()).await;
        self.pool.disconnect().await;
    }
    
    /// Subscribe to pool
    async fn subscribe(&self) -> Result<()> {
        let request = StratumRequest::subscribe(
            &self.config.user_agent,
            self.state.read().await.session_id.as_deref(),
        );
        
        let response = self.pool.send_request(request).await?;
        
        if let Some(error) = response.error {
            return Err(StratumError::JsonRpc {
                code: error.code,
                message: error.message,
            });
        }
        
        if let Some(result) = response.result {
            let subscribe_result = SubscribeResult::from_response(&result)
                .map_err(|e| StratumError::Protocol(e))?;
            
            let mut state = self.state.write().await;
            state.extra_nonce1 = Some(subscribe_result.extra_nonce1);
            state.extra_nonce2_size = subscribe_result.extra_nonce2_size;
            state.session_id = subscribe_result.session_id;
            
            info!("Subscribed to pool successfully");
            Ok(())
        } else {
            Err(StratumError::Protocol("No result in subscribe response".to_string()))
        }
    }
    
    /// Authorize worker
    async fn authorize(&self, username: &str, password: &str) -> Result<()> {
        let request = StratumRequest::authorize(username, password);
        let response = self.pool.send_request(request).await?;
        
        if let Some(error) = response.error {
            return Err(StratumError::AuthenticationFailed(error.message));
        }
        
        if let Some(result) = response.result {
            let authorized = result.as_bool().unwrap_or(false);
            
            if authorized {
                self.state.write().await.authorized = true;
                self.pool.set_authenticated(true).await;
                info!("Worker authorized successfully");
                Ok(())
            } else {
                Err(StratumError::AuthenticationFailed("Pool rejected authorization".to_string()))
            }
        } else {
            Err(StratumError::Protocol("No result in authorize response".to_string()))
        }
    }
    
    /// Submit share to pool
    pub async fn submit_share(&self, share: Share) -> Result<()> {
        // Add to queue
        {
            let mut queue = self.share_queue.lock().await;
            if queue.len() >= self.config.share_buffer_size {
                queue.pop_front(); // Remove oldest share if buffer full
            }
            queue.push_back(share.clone());
        }
        
        // Try to submit immediately if connected
        if self.pool.is_connected().await {
            self.process_share_queue().await;
        }
        
        Ok(())
    }
    
    /// Process queued shares
    async fn process_share_queue(&self) {
        let mut queue = self.share_queue.lock().await;
        let mut processed = Vec::new();
        
        for (idx, share) in queue.iter().enumerate() {
            let request = StratumRequest::submit(share);
            
            match self.pool.send_request(request).await {
                Ok(response) => {
                    if let Some(error) = response.error {
                        warn!("Share rejected: {}", error.message);
                        self.state.write().await.stats.shares_rejected += 1;
                    } else {
                        info!("Share accepted");
                        self.state.write().await.stats.shares_accepted += 1;
                    }
                    processed.push(idx);
                }
                Err(e) => {
                    warn!("Failed to submit share: {}", e);
                    break; // Stop processing on error
                }
            }
        }
        
        // Remove processed shares
        for idx in processed.iter().rev() {
            queue.remove(*idx);
        }
    }
    
    /// Start notification handler task
    async fn start_notification_handler(&self) {
        let notification_rx = self.notification_rx.clone();
        let state = self.state.clone();
        let job_tx = self.job_tx.clone();
        
        tokio::spawn(async move {
            let mut rx = notification_rx.lock().await;
            
            while let Some(notification) = rx.recv().await {
                match notification.method.as_str() {
                    methods::NOTIFY => {
                        match MiningJob::from_notify_params(&notification.params) {
                            Ok(job) => {
                                info!("New mining job: {}", job);
                                
                                // Update state
                                state.write().await.current_job = Some(job.clone());
                                
                                // Send to miners
                                let _ = job_tx.send(job).await;
                            }
                            Err(e) => {
                                error!("Failed to parse mining job: {}", e);
                            }
                        }
                    }
                    methods::SET_DIFFICULTY => {
                        if let Some(Value::Number(n)) = notification.params.first() {
                            if let Some(diff) = n.as_f64() {
                                info!("New difficulty: {}", diff);
                                state.write().await.difficulty = Difficulty(diff);
                            }
                        }
                    }
                    methods::RECONNECT => {
                        warn!("Pool requested reconnection");
                        // TODO: Handle reconnect request
                    }
                    methods::SHOW_MESSAGE => {
                        if let Some(Value::String(msg)) = notification.params.first() {
                            info!("Pool message: {}", msg);
                        }
                    }
                    _ => {
                        debug!("Unknown notification: {}", notification.method);
                    }
                }
            }
        });
    }
    
    /// Start share submission handler
    async fn start_share_handler(&self) {
        let share_queue = self.share_queue.clone();
        let pool = self.pool.clone();
        let config = self.config.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));
            
            loop {
                interval.tick().await;
                
                // Check if we have pending shares and are connected
                if !share_queue.lock().await.is_empty() && pool.is_connected().await {
                    // Process will be handled by process_share_queue
                }
            }
        });
    }
    
    /// Start keepalive task
    async fn start_keepalive(&self) {
        let pool = self.pool.clone();
        let config = self.config.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(config.keepalive_interval);
            
            loop {
                interval.tick().await;
                
                if pool.is_connected().await {
                    // Send ping or get_version as keepalive
                    let request = StratumRequest {
                        id: Some(Value::Number(999999.into())),
                        method: methods::PING.to_string(),
                        params: vec![],
                    };
                    
                    if let Err(e) = pool.send_request(request).await {
                        warn!("Keepalive failed: {}", e);
                    }
                }
            }
        });
    }
    
    /// Get current mining statistics
    pub async fn get_stats(&self) -> MiningStats {
        self.state.read().await.stats.clone()
    }
    
    /// Get current mining job
    pub async fn get_current_job(&self) -> Option<MiningJob> {
        self.state.read().await.current_job.clone()
    }
    
    /// Get current difficulty
    pub async fn get_difficulty(&self) -> Difficulty {
        self.state.read().await.difficulty
    }
}