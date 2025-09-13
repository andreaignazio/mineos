use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::nvml::GpuMetrics;

/// Collects and stores GPU metrics over time
pub struct MetricsCollector {
    history: Arc<Mutex<VecDeque<MetricsSnapshot>>>,
    max_history_size: usize,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(max_history_seconds: u64) -> Self {
        // Assuming 1 sample per second
        let max_history_size = max_history_seconds as usize;
        
        Self {
            history: Arc::new(Mutex::new(VecDeque::with_capacity(max_history_size))),
            max_history_size,
        }
    }
    
    /// Add a new metrics snapshot
    pub fn add_snapshot(&self, metrics: Vec<GpuMetrics>) {
        let snapshot = MetricsSnapshot {
            timestamp: Instant::now(),
            metrics,
        };
        
        let mut history = self.history.lock().unwrap();
        
        // Remove old entries if at capacity
        if history.len() >= self.max_history_size {
            history.pop_front();
        }
        
        history.push_back(snapshot);
    }
    
    /// Get the latest snapshot
    pub fn latest(&self) -> Option<MetricsSnapshot> {
        let history = self.history.lock().unwrap();
        history.back().cloned()
    }
    
    /// Get average metrics over a time period
    pub fn average(&self, duration: Duration) -> Option<AverageMetrics> {
        let history = self.history.lock().unwrap();
        
        if history.is_empty() {
            return None;
        }
        
        let cutoff = Instant::now() - duration;
        let relevant: Vec<_> = history
            .iter()
            .filter(|s| s.timestamp >= cutoff)
            .collect();
        
        if relevant.is_empty() {
            return None;
        }
        
        // Calculate averages for first GPU (extend for multiple GPUs as needed)
        let gpu_count = relevant[0].metrics.len();
        let mut avg_metrics = Vec::new();
        
        for gpu_idx in 0..gpu_count {
            let mut temps = Vec::new();
            let mut power = Vec::new();
            let mut util = Vec::new();
            
            for snapshot in &relevant {
                if let Some(m) = snapshot.metrics.get(gpu_idx) {
                    temps.push(m.temperature as f32);
                    power.push(m.power_usage as f32);
                    util.push(m.gpu_utilization as f32);
                }
            }
            
            if !temps.is_empty() {
                avg_metrics.push(GpuAverageMetrics {
                    index: gpu_idx as u32,
                    avg_temperature: temps.iter().sum::<f32>() / temps.len() as f32,
                    avg_power: power.iter().sum::<f32>() / power.len() as f32,
                    avg_utilization: util.iter().sum::<f32>() / util.len() as f32,
                    max_temperature: temps.iter().cloned().fold(f32::MIN, f32::max),
                    max_power: power.iter().cloned().fold(f32::MIN, f32::max),
                });
            }
        }
        
        Some(AverageMetrics {
            duration,
            sample_count: relevant.len(),
            gpu_metrics: avg_metrics,
        })
    }
    
    /// Clear all history
    pub fn clear(&self) {
        let mut history = self.history.lock().unwrap();
        history.clear();
    }
}

/// A snapshot of GPU metrics at a point in time
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub timestamp: Instant,
    pub metrics: Vec<GpuMetrics>,
}

/// Average metrics over a time period
#[derive(Debug, Clone)]
pub struct AverageMetrics {
    pub duration: Duration,
    pub sample_count: usize,
    pub gpu_metrics: Vec<GpuAverageMetrics>,
}

/// Average metrics for a single GPU
#[derive(Debug, Clone)]
pub struct GpuAverageMetrics {
    pub index: u32,
    pub avg_temperature: f32,
    pub avg_power: f32,
    pub avg_utilization: f32,
    pub max_temperature: f32,
    pub max_power: f32,
}