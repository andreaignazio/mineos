/// Benchmarking framework for MineOS
pub mod hashrate;
pub mod comparison;
pub mod efficiency;
pub mod thermal;
pub mod export;
pub mod suite;
pub mod shares;

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{info, debug, warn};
use chrono::{DateTime, Utc};

pub use hashrate::{HashrateMeter, HashrateStatistics};
pub use comparison::{TRexComparator, ComparisonResults};
pub use efficiency::{PowerMetrics, EfficiencyCalculator};
pub use thermal::{ThermalMonitor, ThermalData};
pub use export::{ExportFormat, JsonExporter, CsvExporter};
pub use suite::{BenchmarkSuite, TestScenario};
pub use shares::{ShareAcceptanceTracker, ShareStats};

/// Benchmark configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    /// Total benchmark duration
    pub duration: Duration,

    /// Warmup time before measurements start
    pub warmup_time: Duration,

    /// Sampling interval for metrics
    pub sample_interval: Duration,

    /// Compare with T-Rex miner
    pub compare_with_trex: bool,

    /// T-Rex API endpoint
    pub trex_api_endpoint: Option<String>,

    /// Export formats
    pub export_formats: Vec<ExportFormat>,

    /// Test scenarios to run
    pub test_scenarios: Vec<TestScenario>,

    /// Enable detailed logging
    pub detailed_logging: bool,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(300), // 5 minutes
            warmup_time: Duration::from_secs(30),
            sample_interval: Duration::from_secs(1),
            compare_with_trex: false,
            trex_api_endpoint: None,
            export_formats: vec![ExportFormat::Json],
            test_scenarios: vec![TestScenario::SteadyState],
            detailed_logging: false,
        }
    }
}

/// Hardware information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub gpu_count: usize,
    pub gpu_models: Vec<String>,
    pub driver_version: String,
    pub cuda_version: Option<String>,
    pub system_memory: u64,
    pub cpu_model: String,
}

/// Complete benchmark results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResults {
    /// Unique session identifier
    pub session_id: String,

    /// Start time of benchmark
    pub start_time: DateTime<Utc>,

    /// End time of benchmark
    pub end_time: DateTime<Utc>,

    /// Hardware configuration
    pub hardware_info: HardwareInfo,

    /// Hashrate statistics
    pub hashrate_stats: HashrateStatistics,

    /// Power consumption metrics
    pub power_metrics: PowerMetrics,

    /// Temperature data
    pub thermal_data: ThermalData,

    /// Share submission statistics
    pub share_statistics: ShareStats,

    /// T-Rex comparison (if enabled)
    pub comparison_data: Option<ComparisonResults>,

    /// Time-series data points
    pub time_series: Vec<MetricSnapshot>,
}

/// Single metric snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSnapshot {
    pub timestamp: DateTime<Utc>,
    pub hashrate_mhs: f64,
    pub power_watts: f32,
    pub temperature_c: f32,
    pub gpu_utilization: f32,
    pub memory_utilization: f32,
    pub shares_accepted: u64,
    pub shares_rejected: u64,
}

/// Main benchmark runner
pub struct BenchmarkRunner {
    config: BenchmarkConfig,
    hashrate_meter: Arc<RwLock<HashrateMeter>>,
    power_calculator: Arc<EfficiencyCalculator>,
    thermal_monitor: Arc<ThermalMonitor>,
    share_tracker: Arc<ShareAcceptanceTracker>,
    comparator: Option<Arc<TRexComparator>>,
    results: Arc<RwLock<Vec<MetricSnapshot>>>,
    pub(crate) start_time: Option<Instant>,
}

impl BenchmarkRunner {
    /// Create a new benchmark runner
    pub fn new(config: BenchmarkConfig) -> Self {
        let comparator = if config.compare_with_trex {
            config.trex_api_endpoint.as_ref().map(|endpoint| {
                Arc::new(TRexComparator::new(endpoint.clone()))
            })
        } else {
            None
        };

        Self {
            config,
            hashrate_meter: Arc::new(RwLock::new(HashrateMeter::new())),
            power_calculator: Arc::new(EfficiencyCalculator::new()),
            thermal_monitor: Arc::new(ThermalMonitor::new()),
            share_tracker: Arc::new(ShareAcceptanceTracker::new()),
            comparator,
            results: Arc::new(RwLock::new(Vec::new())),
            start_time: None,
        }
    }

    /// Start the benchmark
    pub async fn start(&mut self) -> anyhow::Result<()> {
        info!("Starting benchmark with duration: {:?}", self.config.duration);

        // Warmup phase
        if self.config.warmup_time > Duration::ZERO {
            info!("Warming up for {:?}", self.config.warmup_time);
            tokio::time::sleep(self.config.warmup_time).await;
        }

        self.start_time = Some(Instant::now());

        // Start monitoring tasks
        self.start_monitoring().await?;

        Ok(())
    }

    /// Stop the benchmark and collect results
    pub async fn stop(&mut self) -> anyhow::Result<BenchmarkResults> {
        info!("Stopping benchmark and collecting results");

        let end_time = Utc::now();
        let start_time = Utc::now() - chrono::Duration::from_std(
            self.start_time.unwrap_or(Instant::now()).elapsed()
        )?;

        // Collect final statistics
        let hashrate_stats = self.hashrate_meter.read().get_statistics();
        let power_metrics = self.power_calculator.get_metrics();
        let thermal_data = self.thermal_monitor.get_thermal_data();
        let share_statistics = self.share_tracker.get_statistics();

        // Get comparison data if enabled
        let comparison_data = if let Some(ref comparator) = self.comparator {
            Some(comparator.compare(&hashrate_stats, &power_metrics).await?)
        } else {
            None
        };

        let results = BenchmarkResults {
            session_id: uuid::Uuid::new_v4().to_string(),
            start_time,
            end_time,
            hardware_info: self.collect_hardware_info()?,
            hashrate_stats,
            power_metrics,
            thermal_data,
            share_statistics,
            comparison_data,
            time_series: self.results.read().clone(),
        };

        // Export results
        self.export_results(&results).await?;

        Ok(results)
    }

    /// Start monitoring tasks
    async fn start_monitoring(&self) -> anyhow::Result<()> {
        let sample_interval = self.config.sample_interval;
        let results = self.results.clone();
        let hashrate_meter = self.hashrate_meter.clone();
        let thermal_monitor = self.thermal_monitor.clone();
        let share_tracker = self.share_tracker.clone();

        // Spawn monitoring task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(sample_interval);

            loop {
                interval.tick().await;

                // Collect current metrics
                let snapshot = MetricSnapshot {
                    timestamp: Utc::now(),
                    hashrate_mhs: hashrate_meter.read().get_current_hashrate() / 1_000_000.0,
                    power_watts: thermal_monitor.get_current_power(),
                    temperature_c: thermal_monitor.get_current_temperature(),
                    gpu_utilization: 0.0, // Will be filled from GPU monitor
                    memory_utilization: 0.0, // Will be filled from GPU monitor
                    shares_accepted: share_tracker.get_accepted_count(),
                    shares_rejected: share_tracker.get_rejected_count(),
                };

                results.write().push(snapshot);
            }
        });

        Ok(())
    }

    /// Collect hardware information
    fn collect_hardware_info(&self) -> anyhow::Result<HardwareInfo> {
        // This will be filled with actual hardware detection
        Ok(HardwareInfo {
            gpu_count: 1,
            gpu_models: vec!["NVIDIA RTX 3090".to_string()],
            driver_version: "535.104.05".to_string(),
            cuda_version: Some("12.2".to_string()),
            system_memory: 32 * 1024 * 1024 * 1024, // 32GB
            cpu_model: "AMD Ryzen 9 5900X".to_string(),
        })
    }

    /// Export results in configured formats
    async fn export_results(&self, results: &BenchmarkResults) -> anyhow::Result<()> {
        for format in &self.config.export_formats {
            match format {
                ExportFormat::Json => {
                    JsonExporter::export(results, "benchmark_results.json")?;
                    info!("Exported results to benchmark_results.json");
                }
                ExportFormat::Csv => {
                    CsvExporter::export(results, "benchmark_results.csv")?;
                    info!("Exported results to benchmark_results.csv");
                }
                ExportFormat::Markdown => {
                    // TODO: Implement markdown export
                    debug!("Markdown export not yet implemented");
                }
            }
        }

        Ok(())
    }

    /// Update hashrate measurement
    pub fn update_hashrate(&self, gpu_index: usize, hashrate: f64) {
        self.hashrate_meter.write().update(gpu_index, hashrate);
    }

    /// Update share statistics
    pub fn update_shares(&self, accepted: bool) {
        if accepted {
            self.share_tracker.record_accepted();
        } else {
            self.share_tracker.record_rejected();
        }
    }
}