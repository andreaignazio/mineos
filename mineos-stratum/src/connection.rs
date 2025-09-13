use crate::{
    error::{Result, StratumError},
    protocol::{StratumRequest, StratumResponse, StratumNotification},
};
use backoff::{ExponentialBackoff, future::retry};
use futures::{sink::SinkExt, stream::StreamExt};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};
use tokio::{
    net::TcpStream,
    sync::{mpsc, Mutex, RwLock},
    time::{timeout, sleep},
};
use tokio_util::codec::{Framed, LinesCodec};
use tracing::{debug, error, info, warn};

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Authenticated,
    Reconnecting,
    Failed,
}

/// Stratum connection handler
pub struct StratumConnection {
    /// Pool host
    host: String,
    
    /// Pool port
    port: u16,
    
    /// Current connection state
    state: Arc<RwLock<ConnectionState>>,
    
    /// Active TCP stream
    stream: Arc<Mutex<Option<Framed<TcpStream, LinesCodec>>>>,
    
    /// Pending requests waiting for response
    pending_requests: Arc<Mutex<HashMap<Value, mpsc::Sender<StratumResponse>>>>,
    
    /// Channel for sending requests
    request_tx: mpsc::Sender<StratumRequest>,
    request_rx: Arc<Mutex<mpsc::Receiver<StratumRequest>>>,
    
    /// Channel for notifications
    notification_tx: mpsc::Sender<StratumNotification>,
    
    /// Reconnection settings
    max_reconnect_attempts: u32,
    reconnect_backoff_ms: u64,
    max_reconnect_backoff_ms: u64,
    
    /// Connection timeout
    connection_timeout: Duration,
    
    /// Response timeout
    response_timeout: Duration,
}

impl StratumConnection {
    /// Create new connection
    pub fn new(
        host: String,
        port: u16,
        notification_tx: mpsc::Sender<StratumNotification>,
        connection_timeout: Duration,
        response_timeout: Duration,
        max_reconnect_attempts: u32,
        reconnect_backoff_ms: u64,
        max_reconnect_backoff_ms: u64,
    ) -> Self {
        let (request_tx, request_rx) = mpsc::channel(100);
        
        Self {
            host,
            port,
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            stream: Arc::new(Mutex::new(None)),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            request_tx,
            request_rx: Arc::new(Mutex::new(request_rx)),
            notification_tx,
            max_reconnect_attempts,
            reconnect_backoff_ms,
            max_reconnect_backoff_ms,
            connection_timeout,
            response_timeout,
        }
    }
    
    /// Get current connection state
    pub async fn state(&self) -> ConnectionState {
        *self.state.read().await
    }
    
    /// Connect to pool with retry logic
    pub async fn connect(&self) -> Result<()> {
        let backoff = ExponentialBackoff {
            initial_interval: Duration::from_millis(self.reconnect_backoff_ms),
            max_interval: Duration::from_millis(self.max_reconnect_backoff_ms),
            max_elapsed_time: Some(Duration::from_secs(
                self.max_reconnect_attempts as u64 * self.max_reconnect_backoff_ms / 1000
            )),
            ..Default::default()
        };
        
        retry(backoff, || async {
            self.connect_once().await.map_err(|e| {
                warn!("Connection attempt failed: {}", e);
                backoff::Error::transient(e)
            })
        }).await.map_err(|e| {
            error!("All connection attempts failed: {}", e);
            StratumError::Connection(format!("Failed to connect after {} attempts", 
                self.max_reconnect_attempts))
        })
    }
    
    /// Single connection attempt
    async fn connect_once(&self) -> Result<()> {
        info!("Connecting to {}:{}", self.host, self.port);
        *self.state.write().await = ConnectionState::Connecting;
        
        let addr = format!("{}:{}", self.host, self.port);
        let stream = timeout(self.connection_timeout, TcpStream::connect(&addr))
            .await
            .map_err(|_| StratumError::Timeout)?
            .map_err(|e| StratumError::Connection(e.to_string()))?;
        
        stream.set_nodelay(true)
            .map_err(|e| StratumError::Connection(e.to_string()))?;
        
        let framed = Framed::new(stream, LinesCodec::new());
        *self.stream.lock().await = Some(framed);
        *self.state.write().await = ConnectionState::Connected;
        
        info!("Connected to {}", addr);
        
        // Start read/write tasks
        self.start_tasks().await;
        
        Ok(())
    }
    
    /// Disconnect from pool
    pub async fn disconnect(&self) {
        info!("Disconnecting from pool");
        *self.state.write().await = ConnectionState::Disconnected;
        
        if let Some(mut stream) = self.stream.lock().await.take() {
            let _ = SinkExt::<String>::close(&mut stream).await;
        }
        
        // Clear pending requests
        self.pending_requests.lock().await.clear();
    }
    
    /// Send request and wait for response
    pub async fn send_request(&self, request: StratumRequest) -> Result<StratumResponse> {
        // Check connection state
        let state = self.state().await;
        if state != ConnectionState::Connected && state != ConnectionState::Authenticated {
            return Err(StratumError::Connection("Not connected".to_string()));
        }
        
        // Create response channel if request has ID
        let response_rx = if let Some(ref id) = request.id {
            let (tx, rx) = mpsc::channel(1);
            self.pending_requests.lock().await.insert(id.clone(), tx);
            Some(rx)
        } else {
            None
        };
        
        // Send request
        self.request_tx.send(request.clone()).await
            .map_err(|_| StratumError::Connection("Failed to send request".to_string()))?;
        
        // Wait for response if request had ID
        if let Some(mut rx) = response_rx {
            match timeout(self.response_timeout, rx.recv()).await {
                Ok(Some(response)) => Ok(response),
                Ok(None) => Err(StratumError::Connection("Response channel closed".to_string())),
                Err(_) => {
                    // Remove from pending on timeout
                    if let Some(ref id) = request.id {
                        self.pending_requests.lock().await.remove(id);
                    }
                    Err(StratumError::Timeout)
                }
            }
        } else {
            // No response expected for notifications
            Ok(StratumResponse {
                id: None,
                result: None,
                error: None,
            })
        }
    }
    
    /// Start read and write tasks
    async fn start_tasks(&self) {
        let stream = self.stream.clone();
        let pending = self.pending_requests.clone();
        let notification_tx = self.notification_tx.clone();
        let state = self.state.clone();
        
        // Read task
        tokio::spawn(async move {
            if let Some(mut framed) = stream.lock().await.take() {
                while let Some(result) = framed.next().await {
                    match result {
                        Ok(line) => {
                            debug!("Received: {}", line);
                            
                            // Try to parse as response
                            if let Ok(response) = serde_json::from_str::<StratumResponse>(&line) {
                                if let Some(ref id) = response.id {
                                    if let Some(tx) = pending.lock().await.remove(id) {
                                        let _ = tx.send(response).await;
                                    }
                                }
                            }
                            // Try to parse as notification
                            else if let Ok(notification) = serde_json::from_str::<StratumNotification>(&line) {
                                let _ = notification_tx.send(notification).await;
                            }
                            else {
                                warn!("Failed to parse message: {}", line);
                            }
                        }
                        Err(e) => {
                            error!("Read error: {}", e);
                            *state.write().await = ConnectionState::Disconnected;
                            break;
                        }
                    }
                }
                
                // Put stream back for potential reuse
                *stream.lock().await = Some(framed);
            }
        });
        
        // Write task
        let stream = self.stream.clone();
        let request_rx = self.request_rx.clone();
        let state = self.state.clone();
        
        tokio::spawn(async move {
            let mut rx = request_rx.lock().await;
            
            while let Some(request) = rx.recv().await {
                if let Some(framed) = stream.lock().await.as_mut() {
                    let msg = serde_json::to_string(&request).unwrap() + "\n";
                    debug!("Sending: {}", msg.trim());
                    
                    if let Err(e) = SinkExt::<String>::send(framed, msg).await {
                        error!("Write error: {}", e);
                        *state.write().await = ConnectionState::Disconnected;
                        break;
                    }
                }
            }
        });
    }
    
    /// Handle reconnection
    pub async fn reconnect(&self) -> Result<()> {
        *self.state.write().await = ConnectionState::Reconnecting;
        
        // Disconnect first
        self.disconnect().await;
        
        // Wait a bit before reconnecting
        sleep(Duration::from_millis(self.reconnect_backoff_ms)).await;
        
        // Try to connect again
        self.connect().await
    }
    
    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        let state = self.state().await;
        state == ConnectionState::Connected || state == ConnectionState::Authenticated
    }
    
    /// Set authenticated state
    pub async fn set_authenticated(&self, authenticated: bool) {
        if authenticated {
            *self.state.write().await = ConnectionState::Authenticated;
        } else {
            *self.state.write().await = ConnectionState::Connected;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_connection_state() {
        let (tx, _rx) = mpsc::channel(10);
        let conn = StratumConnection::new(
            "test.pool.com".to_string(),
            3333,
            tx,
            Duration::from_secs(5),
            Duration::from_secs(10),
            3,
            1000,
            30000,
        );
        
        assert_eq!(conn.state().await, ConnectionState::Disconnected);
    }
}