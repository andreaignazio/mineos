use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use mineos_hardware::manager::GpuManager;
use mineos_hardware::monitor::GpuMetrics;

/// Performance metrics for mining operations
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Total hashrate across all GPUs (H/s)
    pub total_hashrate: f64,

    /// Per-GPU hashrates
    pub gpu_hashrates: HashMap<usize, f64>,

    /// Average GPU utilization (%)
    pub avg_gpu_utilization: f32,

    /// Average GPU temperature (Celsius)
    pub avg_gpu_temperature: f32,

    /// Total power consumption (Watts)
    pub total_power_watts: f32,

    /// Mining efficiency (H/W)
    pub efficiency_hw: f64,

    /// Valid shares per minute
    pub shares_per_minute: f64,

    /// Share acceptance rate (%)
    pub acceptance_rate: f64,

    /// Average work unit completion time
    pub avg_work_time: Duration,

    /// Work units per minute
    pub work_units_per_minute: f64,

    /// Timestamp of metrics
    pub timestamp: Instant,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            total_hashrate: 0.0,
            gpu_hashrates: HashMap::new(),
            avg_gpu_utilization: 0.0,
            avg_gpu_temperature: 0.0,
            total_power_watts: 0.0,
            efficiency_hw: 0.0,
            shares_per_minute: 0.0,
            acceptance_rate: 100.0,
            avg_work_time: Duration::from_secs(0),
            work_units_per_minute: 0.0,
            timestamp: Instant::now(),
        }
    }
}

/// Historical data point
#[derive(Debug, Clone)]
pub struct MetricSnapshot {
    pub timestamp: Instant,
    pub hashrate: f64,
    pub temperature: f32,
    pub power: f32,
    pub shares: u64,
}

/// Configuration for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Update interval for metrics
    pub update_interval: Duration,

    /// History retention period
    pub history_duration: Duration,

    /// Maximum history samples
    pub max_history_samples: usize,

    /// Enable detailed GPU monitoring
    pub detailed_gpu_monitoring: bool,

    /// Alert thresholds
    pub alert_thresholds: AlertThresholds,
}

/// Alert thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Minimum hashrate (% of expected)
    pub min_hashrate_percent: f64,

    /// Maximum temperature (Celsius)
    pub max_temperature: f32,

    /// Maximum power (Watts)
    pub max_power: f32,

    /// Minimum GPU utilization (%)
    pub min_utilization: f32,

    /// Maximum rejected share rate (%)
    pub max_reject_rate: f64,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            update_interval: Duration::from_secs(5),
            history_duration: Duration::from_secs(3600), // 1 hour
            max_history_samples: 720, // 5-second intervals for 1 hour
            detailed_gpu_monitoring: true,
            alert_thresholds: AlertThresholds {
                min_hashrate_percent: 90.0,
                max_temperature: 85.0,
                max_power: 350.0,
                min_utilization: 90.0,
                max_reject_rate: 5.0,
            },
        }
    }
}

/// Monitors GPU utilization and performance
pub struct GpuUtilizationMonitor {
    /// Configuration
    config: MonitoringConfig,

    /// Current metrics
    current_metrics: Arc<RwLock<PerformanceMetrics>>,

    /// Historical metrics
    history: RwLock<VecDeque<MetricSnapshot>>,

    /// GPU manager reference
    gpu_manager: Option<Arc<GpuManager>>,

    /// Alert state
    alerts: RwLock<Vec<Alert>>,

    /// Statistics
    stats: RwLock<MonitoringStats>,

    /// Last update time
    last_update: RwLock<Instant>,

    /// Expected hashrates per GPU
    expected_hashrates: RwLock<HashMap<usize, f64>>,
}

/// Active alert
#[derive(Debug, Clone)]
pub struct Alert {
    pub alert_type: AlertType,
    pub gpu_index: Option<usize>,
    pub message: String,
    pub severity: AlertSeverity,
    pub timestamp: Instant,
}

/// Alert types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertType {
    LowHashrate,
    HighTemperature,
    HighPower,
    LowUtilization,
    HighRejectRate,
    GpuOffline,
}

/// Alert severity
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

/// Monitoring statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MonitoringStats {
    pub total_updates: u64,
    pub alerts_triggered: u64,
    pub peak_hashrate: f64,
    pub peak_temperature: f32,
    pub peak_power: f32,
    pub total_runtime: Duration,
}

impl GpuUtilizationMonitor {
    /// Create a new GPU utilization monitor
    pub fn new(config: MonitoringConfig) -> Self {
        Self {
            config,
            current_metrics: Arc::new(RwLock::new(PerformanceMetrics::default())),
            history: RwLock::new(VecDeque::with_capacity(1000)),
            gpu_manager: None,
            alerts: RwLock::new(Vec::new()),
            stats: RwLock::new(MonitoringStats::default()),
            last_update: RwLock::new(Instant::now()),
            expected_hashrates: RwLock::new(HashMap::new()),
        }
    }

    /// Set the GPU manager for hardware monitoring
    pub fn set_gpu_manager(&mut self, manager: Arc<GpuManager>) {
        self.gpu_manager = Some(manager);
    }

    /// Set expected hashrate for a GPU
    pub fn set_expected_hashrate(&self, gpu_index: usize, hashrate: f64) {
        let mut expected = self.expected_hashrates.write();
        expected.insert(gpu_index, hashrate);
    }

    /// Update metrics with current data
    pub fn update_metrics(
        &self,
        gpu_hashrates: HashMap<usize, f64>,
        shares_accepted: u64,
        shares_rejected: u64,
        work_units_completed: u64,
        avg_work_time: Duration,
    ) {
        let now = Instant::now();
        let elapsed = now.duration_since(*self.last_update.read());

        // Calculate rates
        let shares_per_minute = if elapsed.as_secs() > 0 {
            (shares_accepted as f64 / elapsed.as_secs_f64()) * 60.0
        } else {
            0.0
        };

        let work_units_per_minute = if elapsed.as_secs() > 0 {
            (work_units_completed as f64 / elapsed.as_secs_f64()) * 60.0
        } else {
            0.0
        };

        let acceptance_rate = if shares_accepted + shares_rejected > 0 {
            (shares_accepted as f64 / (shares_accepted + shares_rejected) as f64) * 100.0
        } else {
            100.0
        };

        // Get hardware metrics if available
        let (gpu_metrics, avg_util, avg_temp, total_power) = self.get_hardware_metrics();

        // Calculate total hashrate
        let total_hashrate: f64 = gpu_hashrates.values().sum();

        // Calculate efficiency
        let efficiency_hw = if total_power > 0.0 {
            total_hashrate / total_power as f64
        } else {
            0.0
        };

        // Update current metrics
        let metrics = PerformanceMetrics {
            total_hashrate,
            gpu_hashrates: gpu_hashrates.clone(),
            avg_gpu_utilization: avg_util,
            avg_gpu_temperature: avg_temp,
            total_power_watts: total_power,
            efficiency_hw,
            shares_per_minute,
            acceptance_rate,
            avg_work_time,
            work_units_per_minute,
            timestamp: now,
        };

        *self.current_metrics.write() = metrics.clone();

        // Add to history
        self.add_to_history(metrics.clone());

        // Check for alerts
        self.check_alerts(&metrics, &gpu_metrics);

        // Update statistics
        self.update_stats(&metrics);

        *self.last_update.write() = now;

        debug!("Updated metrics: {:.2} MH/s, {:.1}°C, {:.0}W, {:.1}% acceptance",
               total_hashrate / 1_000_000.0, avg_temp, total_power, acceptance_rate);
    }

    /// Get hardware metrics from GPU manager
    fn get_hardware_metrics(&self) -> (Vec<GpuMetrics>, f32, f32, f32) {
        if let Some(ref gpu_manager) = self.gpu_manager {
            let gpu_metrics = gpu_manager.get_metrics();

            if !gpu_metrics.is_empty() {
                let avg_util = gpu_metrics.iter()
                    .map(|m| m.gpu_utilization)
                    .sum::<u32>() as f32 / gpu_metrics.len() as f32;

                let avg_temp = gpu_metrics.iter()
                    .map(|m| m.temperature)
                    .sum::<u32>() as f32 / gpu_metrics.len() as f32;

                let total_power = gpu_metrics.iter()
                    .map(|m| m.power_usage as f32)
                    .sum::<f32>();

                return (gpu_metrics, avg_util, avg_temp, total_power);
            }
        }

        (Vec::new(), 0.0, 0.0, 0.0)
    }

    /// Add metrics to history
    fn add_to_history(&self, metrics: PerformanceMetrics) {
        let mut history = self.history.write();

        // Create snapshot
        let snapshot = MetricSnapshot {
            timestamp: metrics.timestamp,
            hashrate: metrics.total_hashrate,
            temperature: metrics.avg_gpu_temperature,
            power: metrics.total_power_watts,
            shares: (metrics.shares_per_minute * 60.0) as u64,
        };

        history.push_back(snapshot);

        // Remove old entries
        let cutoff = Instant::now() - self.config.history_duration;
        while let Some(front) = history.front() {
            if front.timestamp < cutoff || history.len() > self.config.max_history_samples {
                history.pop_front();
            } else {
                break;
            }
        }
    }

    /// Check for alert conditions
    fn check_alerts(&self, metrics: &PerformanceMetrics, gpu_metrics: &[GpuMetrics]) {
        let mut alerts = self.alerts.write();
        alerts.clear();

        // Check hashrate
        let expected_total: f64 = self.expected_hashrates.read().values().sum();
        if expected_total > 0.0 {
            let hashrate_percent = (metrics.total_hashrate / expected_total) * 100.0;
            if hashrate_percent < self.config.alert_thresholds.min_hashrate_percent {
                alerts.push(Alert {
                    alert_type: AlertType::LowHashrate,
                    gpu_index: None,
                    message: format!("Total hashrate {:.1}% of expected", hashrate_percent),
                    severity: AlertSeverity::Warning,
                    timestamp: Instant::now(),
                });
            }
        }

        // Check temperature
        if metrics.avg_gpu_temperature > self.config.alert_thresholds.max_temperature {
            alerts.push(Alert {
                alert_type: AlertType::HighTemperature,
                gpu_index: None,
                message: format!("Average GPU temperature {:.1}°C", metrics.avg_gpu_temperature),
                severity: AlertSeverity::Critical,
                timestamp: Instant::now(),
            });
        }

        // Check power
        if metrics.total_power_watts > self.config.alert_thresholds.max_power * gpu_metrics.len() as f32 {
            alerts.push(Alert {
                alert_type: AlertType::HighPower,
                gpu_index: None,
                message: format!("Total power draw {:.0}W", metrics.total_power_watts),
                severity: AlertSeverity::Warning,
                timestamp: Instant::now(),
            });
        }

        // Check utilization
        if metrics.avg_gpu_utilization < self.config.alert_thresholds.min_utilization {
            alerts.push(Alert {
                alert_type: AlertType::LowUtilization,
                gpu_index: None,
                message: format!("Average GPU utilization {:.1}%", metrics.avg_gpu_utilization),
                severity: AlertSeverity::Warning,
                timestamp: Instant::now(),
            });
        }

        // Check reject rate
        if metrics.acceptance_rate < (100.0 - self.config.alert_thresholds.max_reject_rate) {
            alerts.push(Alert {
                alert_type: AlertType::HighRejectRate,
                gpu_index: None,
                message: format!("Share acceptance rate {:.1}%", metrics.acceptance_rate),
                severity: AlertSeverity::Warning,
                timestamp: Instant::now(),
            });
        }

        // Per-GPU checks
        for (i, gpu_metric) in gpu_metrics.iter().enumerate() {
            if gpu_metric.temperature > self.config.alert_thresholds.max_temperature as u32 {
                alerts.push(Alert {
                    alert_type: AlertType::HighTemperature,
                    gpu_index: Some(i),
                    message: format!("GPU {} temperature {}°C", i, gpu_metric.temperature),
                    severity: AlertSeverity::Critical,
                    timestamp: Instant::now(),
                });
            }
        }

        if !alerts.is_empty() {
            let mut stats = self.stats.write();
            stats.alerts_triggered += alerts.len() as u64;

            for alert in alerts.iter() {
                warn!("[{:?}] {}", alert.severity, alert.message);
            }
        }
    }

    /// Update statistics
    fn update_stats(&self, metrics: &PerformanceMetrics) {
        let mut stats = self.stats.write();
        stats.total_updates += 1;

        if metrics.total_hashrate > stats.peak_hashrate {
            stats.peak_hashrate = metrics.total_hashrate;
        }

        if metrics.avg_gpu_temperature > stats.peak_temperature {
            stats.peak_temperature = metrics.avg_gpu_temperature;
        }

        if metrics.total_power_watts > stats.peak_power {
            stats.peak_power = metrics.total_power_watts;
        }
    }

    /// Get current metrics
    pub fn get_current_metrics(&self) -> PerformanceMetrics {
        self.current_metrics.read().clone()
    }

    /// Get historical data
    pub fn get_history(&self) -> Vec<MetricSnapshot> {
        self.history.read().iter().cloned().collect()
    }

    /// Get active alerts
    pub fn get_alerts(&self) -> Vec<Alert> {
        self.alerts.read().clone()
    }

    /// Get monitoring statistics
    pub fn get_stats(&self) -> MonitoringStats {
        self.stats.read().clone()
    }

    /// Calculate average metrics over a time period
    pub fn calculate_averages(&self, duration: Duration) -> Option<PerformanceMetrics> {
        let history = self.history.read();
        let cutoff = Instant::now() - duration;

        let recent: Vec<_> = history.iter()
            .filter(|s| s.timestamp > cutoff)
            .collect();

        if recent.is_empty() {
            return None;
        }

        let avg_hashrate = recent.iter().map(|s| s.hashrate).sum::<f64>() / recent.len() as f64;
        let avg_temp = recent.iter().map(|s| s.temperature).sum::<f32>() / recent.len() as f32;
        let avg_power = recent.iter().map(|s| s.power).sum::<f32>() / recent.len() as f32;

        Some(PerformanceMetrics {
            total_hashrate: avg_hashrate,
            avg_gpu_temperature: avg_temp,
            total_power_watts: avg_power,
            efficiency_hw: if avg_power > 0.0 { avg_hashrate / avg_power as f64 } else { 0.0 },
            ..Default::default()
        })
    }
}