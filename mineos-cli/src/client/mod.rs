use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use once_cell::sync::OnceCell;

use crate::dashboard::widgets::{GpuStats, MinerStatus, LogEntry};
use crate::miner_service::MinerService;
use crate::config::MinerConfig;

// Global miner service instance
static MINER_SERVICE: OnceCell<Arc<RwLock<Option<MinerService>>>> = OnceCell::new();

/// Client for communicating with the miner process
pub struct MinerClient {
    service: Arc<RwLock<Option<MinerService>>>,
}

impl MinerClient {
    /// Connect to or create the miner service
    pub async fn connect() -> Result<Self> {
        let service = MINER_SERVICE.get_or_init(|| {
            Arc::new(RwLock::new(None))
        }).clone();

        Ok(Self { service })
    }

    /// Initialize the miner service with config
    pub async fn initialize(&mut self, config: MinerConfig) -> Result<()> {
        let miner_service = MinerService::new(config).await?;
        let mut service = self.service.write().await;
        *service = Some(miner_service);
        Ok(())
    }

    /// Check if miner is running
    pub async fn is_running() -> bool {
        if let Some(service) = MINER_SERVICE.get() {
            let guard = service.read().await;
            guard.is_some()
        } else {
            false
        }
    }

    /// Get GPU statistics
    pub async fn get_gpu_statistics(&self) -> Result<Vec<GpuStats>> {
        let service = self.service.read().await;
        match &*service {
            Some(miner) => miner.get_gpu_statistics().await,
            None => Err(anyhow::anyhow!("Miner service not initialized")),
        }
    }

    /// Get miner status
    pub async fn get_status(&self) -> Result<MinerStatus> {
        let service = self.service.read().await;
        match &*service {
            Some(miner) => miner.get_status().await,
            None => {
                // Return default status when not initialized
                Ok(MinerStatus {
                    total_shares: 0,
                    accepted_shares: 0,
                    rejected_shares: 0,
                    stale_shares: 0,
                    total_hashrate: 0.0,
                    pool_connected: false,
                    is_mining: false,
                    algorithm: "none".to_string(),
                    total_hashrate_mhs: 0.0,
                    active_gpus: 0,
                    uptime_seconds: 0,
                })
            }
        }
    }

    /// Get recent logs
    pub async fn get_recent_logs(&self, count: usize) -> Result<Vec<LogEntry>> {
        let service = self.service.read().await;
        match &*service {
            Some(miner) => miner.get_recent_logs(count).await,
            None => Ok(Vec::new()),
        }
    }

    /// Start mining
    pub async fn start_mining(&mut self, config: MinerConfig) -> Result<()> {
        // Initialize if not already done
        if self.service.read().await.is_none() {
            self.initialize(config.clone()).await?;
        }

        let service = self.service.read().await;
        match &*service {
            Some(miner) => miner.start_mining().await,
            None => Err(anyhow::anyhow!("Failed to initialize miner service")),
        }
    }

    /// Stop the miner
    pub async fn stop(&mut self) -> Result<()> {
        let service = self.service.read().await;
        match &*service {
            Some(miner) => miner.stop_mining().await,
            None => Err(anyhow::anyhow!("Miner service not running")),
        }
    }

    /// Pause mining
    pub async fn pause(&mut self) -> Result<()> {
        // In production, this would pause without disconnecting
        self.stop().await
    }

    /// Resume mining
    pub async fn resume(&mut self) -> Result<()> {
        let service = self.service.read().await;
        match &*service {
            Some(miner) => miner.start_mining().await,
            None => Err(anyhow::anyhow!("Miner service not initialized")),
        }
    }

    /// Run benchmark
    pub async fn run_benchmark(&self, duration_secs: u64) -> Result<crate::miner_service::BenchmarkResults> {
        let service = self.service.read().await;
        match &*service {
            Some(miner) => miner.run_benchmark(duration_secs).await,
            None => Err(anyhow::anyhow!("Miner service not initialized")),
        }
    }
}

/// Commands that can be sent to the miner
#[derive(Debug, Serialize, Deserialize)]
pub enum MinerCommand {
    Stop,
    Pause,
    Resume,
    GetStatus,
    GetGpuStats,
    GetLogs { count: usize },
    SetOverclock { gpu_index: usize, core: i32, memory: i32, power: u32 },
}

/// Response from the miner
#[derive(Debug, Serialize, Deserialize)]
pub enum MinerResponse {
    Ok,
    Error { message: String },
    Status(MinerStatus),
    GpuStats(Vec<GpuStats>),
    Logs(Vec<LogEntry>),
}