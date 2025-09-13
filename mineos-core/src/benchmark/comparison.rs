/// T-Rex miner comparison engine
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use reqwest;

use super::hashrate::HashrateStatistics;
use super::efficiency::PowerMetrics;

/// Miner performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinerMetrics {
    /// Miner name
    pub name: String,

    /// Average hashrate (H/s)
    pub hashrate: f64,

    /// Power consumption (W)
    pub power_watts: f32,

    /// Efficiency (H/W)
    pub efficiency: f64,

    /// Share acceptance rate (%)
    pub acceptance_rate: f64,

    /// Average temperature (C)
    pub temperature: f32,

    /// Stability score (0-100)
    pub stability_score: f64,

    /// Per-GPU metrics
    pub gpu_metrics: Vec<GpuMetrics>,
}

/// Per-GPU comparison metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuMetrics {
    pub index: usize,
    pub hashrate: f64,
    pub power: f32,
    pub temperature: f32,
    pub efficiency: f64,
}

/// Comparison results between miners
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonResults {
    /// MineOS metrics
    pub mineos: MinerMetrics,

    /// T-Rex metrics
    pub trex: MinerMetrics,

    /// Performance difference (% - positive means MineOS is better)
    pub performance_delta: f64,

    /// Efficiency difference (% - positive means MineOS is better)
    pub efficiency_delta: f64,

    /// Stability comparison
    pub stability_score: f64,

    /// Detailed comparison
    pub details: ComparisonDetails,
}

/// Detailed comparison breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonDetails {
    /// Hashrate advantage
    pub hashrate_advantage: f64,

    /// Power savings
    pub power_savings: f32,

    /// Temperature difference
    pub temp_difference: f32,

    /// Share acceptance difference
    pub acceptance_difference: f64,

    /// Per-GPU comparison
    pub gpu_comparison: Vec<GpuComparison>,
}

/// Per-GPU comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuComparison {
    pub gpu_index: usize,
    pub mineos_hashrate: f64,
    pub trex_hashrate: f64,
    pub hashrate_diff: f64,
    pub efficiency_diff: f64,
}

/// T-Rex API response structures
#[derive(Debug, Clone, Deserialize)]
struct TRexApiResponse {
    pub hashrate: f64,
    pub hashrate_minute: f64,
    pub hashrate_hour: f64,
    pub accepted_count: u64,
    pub rejected_count: u64,
    pub gpu_total_count: u32,
    pub gpus: Vec<TRexGpu>,
}

#[derive(Debug, Clone, Deserialize)]
struct TRexGpu {
    pub gpu_id: u32,
    pub hashrate: f64,
    pub hashrate_minute: f64,
    pub power: f32,
    pub temperature: u32,
    pub fan_speed: u32,
    pub efficiency: String,
}

/// T-Rex comparison engine
pub struct TRexComparator {
    /// T-Rex API endpoint
    api_endpoint: String,

    /// HTTP client
    client: reqwest::Client,

    /// Cached T-Rex data
    cached_data: Option<TRexApiResponse>,
}

impl TRexComparator {
    /// Create a new T-Rex comparator
    pub fn new(api_endpoint: String) -> Self {
        Self {
            api_endpoint,
            client: reqwest::Client::new(),
            cached_data: None,
        }
    }

    /// Fetch current T-Rex statistics
    pub async fn fetch_trex_stats(&self) -> Result<TRexApiResponse> {
        let url = format!("{}/summary", self.api_endpoint);

        let response = self.client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await?
            .json::<TRexApiResponse>()
            .await?;

        // Note: Cannot update cached_data in non-mutable self
        Ok(response)
    }

    /// Compare MineOS with T-Rex
    pub async fn compare(
        &self,
        mineos_stats: &HashrateStatistics,
        mineos_power: &PowerMetrics,
    ) -> Result<ComparisonResults> {
        // Fetch T-Rex stats
        let trex_data = self.fetch_trex_stats().await?;

        // Build MineOS metrics
        let mineos_metrics = MinerMetrics {
            name: "MineOS".to_string(),
            hashrate: mineos_stats.current,
            power_watts: mineos_power.total_power,
            efficiency: if mineos_power.total_power > 0.0 {
                mineos_stats.current / mineos_power.total_power as f64
            } else {
                0.0
            },
            acceptance_rate: mineos_stats.efficiency_ratio * 100.0,
            temperature: mineos_power.avg_temperature,
            stability_score: Self::calculate_stability(mineos_stats),
            gpu_metrics: Self::build_gpu_metrics_mineos(mineos_stats, mineos_power),
        };

        // Build T-Rex metrics
        let trex_metrics = Self::build_trex_metrics(&trex_data);

        // Calculate deltas
        let performance_delta = if trex_metrics.hashrate > 0.0 {
            ((mineos_metrics.hashrate - trex_metrics.hashrate) / trex_metrics.hashrate) * 100.0
        } else {
            0.0
        };

        let efficiency_delta = if trex_metrics.efficiency > 0.0 {
            ((mineos_metrics.efficiency - trex_metrics.efficiency) / trex_metrics.efficiency) * 100.0
        } else {
            0.0
        };

        // Calculate stability score before moving values
        let stability_score = (mineos_metrics.stability_score + trex_metrics.stability_score) / 2.0;

        // Build detailed comparison
        let details = ComparisonDetails {
            hashrate_advantage: mineos_metrics.hashrate - trex_metrics.hashrate,
            power_savings: trex_metrics.power_watts - mineos_metrics.power_watts,
            temp_difference: trex_metrics.temperature - mineos_metrics.temperature,
            acceptance_difference: mineos_metrics.acceptance_rate - trex_metrics.acceptance_rate,
            gpu_comparison: Self::compare_gpus(&mineos_metrics, &trex_metrics),
        };

        Ok(ComparisonResults {
            mineos: mineos_metrics,
            trex: trex_metrics,
            performance_delta,
            efficiency_delta,
            stability_score,
            details,
        })
    }

    /// Build T-Rex metrics from API response
    fn build_trex_metrics(data: &TRexApiResponse) -> MinerMetrics {
        let total_power: f32 = data.gpus.iter().map(|g| g.power).sum();
        let avg_temp: f32 = if !data.gpus.is_empty() {
            data.gpus.iter().map(|g| g.temperature as f32).sum::<f32>() / data.gpus.len() as f32
        } else {
            0.0
        };

        let acceptance_rate = if data.accepted_count + data.rejected_count > 0 {
            (data.accepted_count as f64 / (data.accepted_count + data.rejected_count) as f64) * 100.0
        } else {
            0.0
        };

        let gpu_metrics: Vec<GpuMetrics> = data.gpus.iter().map(|gpu| {
            GpuMetrics {
                index: gpu.gpu_id as usize,
                hashrate: gpu.hashrate,
                power: gpu.power,
                temperature: gpu.temperature as f32,
                efficiency: if gpu.power > 0.0 {
                    gpu.hashrate / gpu.power as f64
                } else {
                    0.0
                },
            }
        }).collect();

        MinerMetrics {
            name: "T-Rex".to_string(),
            hashrate: data.hashrate,
            power_watts: total_power,
            efficiency: if total_power > 0.0 {
                data.hashrate / total_power as f64
            } else {
                0.0
            },
            acceptance_rate,
            temperature: avg_temp,
            stability_score: Self::calculate_trex_stability(data),
            gpu_metrics,
        }
    }

    /// Build GPU metrics for MineOS
    fn build_gpu_metrics_mineos(
        stats: &HashrateStatistics,
        power: &PowerMetrics,
    ) -> Vec<GpuMetrics> {
        stats.gpu_hashrates.iter().map(|(index, hashrate)| {
            let gpu_power = power.gpu_power.get(index).copied().unwrap_or(0.0);
            GpuMetrics {
                index: *index,
                hashrate: *hashrate,
                power: gpu_power,
                temperature: power.gpu_temperatures.get(index).copied().unwrap_or(0.0),
                efficiency: if gpu_power > 0.0 {
                    hashrate / gpu_power as f64
                } else {
                    0.0
                },
            }
        }).collect()
    }

    /// Calculate stability score for MineOS
    fn calculate_stability(stats: &HashrateStatistics) -> f64 {
        if stats.average == 0.0 {
            return 0.0;
        }

        // Lower coefficient of variation = higher stability
        let cv = stats.std_deviation / stats.average;
        let stability = (1.0 - cv.min(1.0)) * 100.0;

        stability
    }

    /// Calculate stability score for T-Rex
    fn calculate_trex_stability(data: &TRexApiResponse) -> f64 {
        // Compare minute and hour averages to current
        if data.hashrate == 0.0 {
            return 0.0;
        }

        let minute_diff = (data.hashrate - data.hashrate_minute).abs() / data.hashrate;
        let hour_diff = (data.hashrate - data.hashrate_hour).abs() / data.hashrate;

        let avg_diff = (minute_diff + hour_diff) / 2.0;
        let stability = (1.0 - avg_diff.min(1.0)) * 100.0;

        stability
    }

    /// Compare GPU metrics
    fn compare_gpus(mineos: &MinerMetrics, trex: &MinerMetrics) -> Vec<GpuComparison> {
        let mut comparisons = Vec::new();

        for mineos_gpu in &mineos.gpu_metrics {
            if let Some(trex_gpu) = trex.gpu_metrics.iter().find(|g| g.index == mineos_gpu.index) {
                let hashrate_diff = if trex_gpu.hashrate > 0.0 {
                    ((mineos_gpu.hashrate - trex_gpu.hashrate) / trex_gpu.hashrate) * 100.0
                } else {
                    0.0
                };

                let efficiency_diff = if trex_gpu.efficiency > 0.0 {
                    ((mineos_gpu.efficiency - trex_gpu.efficiency) / trex_gpu.efficiency) * 100.0
                } else {
                    0.0
                };

                comparisons.push(GpuComparison {
                    gpu_index: mineos_gpu.index,
                    mineos_hashrate: mineos_gpu.hashrate,
                    trex_hashrate: trex_gpu.hashrate,
                    hashrate_diff,
                    efficiency_diff,
                });
            }
        }

        comparisons
    }
}

/// Benchmark runner for automated comparison
pub struct BenchmarkRunner {
    /// Test duration for each miner
    test_duration: std::time::Duration,

    /// Number of test runs
    num_runs: usize,

    /// Rest time between tests
    rest_time: std::time::Duration,
}

impl BenchmarkRunner {
    /// Create a new benchmark runner
    pub fn new(
        test_duration: std::time::Duration,
        num_runs: usize,
        rest_time: std::time::Duration,
    ) -> Self {
        Self {
            test_duration,
            num_runs,
            rest_time,
        }
    }

    /// Run automated comparison benchmark
    pub async fn run_comparison(&self) -> Result<Vec<ComparisonResults>> {
        let mut results = Vec::new();

        for run in 0..self.num_runs {
            tracing::info!("Starting benchmark run {}/{}", run + 1, self.num_runs);

            // Run MineOS benchmark
            // TODO: Implement actual MineOS benchmark run

            // Rest period
            tokio::time::sleep(self.rest_time).await;

            // Run T-Rex benchmark
            // TODO: Implement T-Rex benchmark run

            // Collect and compare results
            // TODO: Collect actual results

            // Rest before next run
            if run < self.num_runs - 1 {
                tokio::time::sleep(self.rest_time).await;
            }
        }

        Ok(results)
    }
}