/// Hashrate measurement and statistics
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

/// Hashrate statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashrateStatistics {
    /// Current hashrate (H/s)
    pub current: f64,

    /// 5-second average
    pub avg_5s: f64,

    /// 30-second average
    pub avg_30s: f64,

    /// 5-minute average
    pub avg_5m: f64,

    /// 15-minute average
    pub avg_15m: f64,

    /// Minimum hashrate observed
    pub min: f64,

    /// Maximum hashrate observed
    pub max: f64,

    /// Average hashrate over entire period
    pub average: f64,

    /// Standard deviation
    pub std_deviation: f64,

    /// Per-GPU hashrates
    pub gpu_hashrates: HashMap<usize, f64>,

    /// Effective hashrate (based on accepted shares)
    pub effective: f64,

    /// Reported vs effective ratio
    pub efficiency_ratio: f64,
}

/// Time-series data point
#[derive(Debug, Clone)]
struct HashrateDataPoint {
    timestamp: Instant,
    hashrate: f64,
    gpu_index: usize,
}

/// Moving average calculator
pub(super) struct MovingAverage {
    window: Duration,
    data: VecDeque<HashrateDataPoint>,
}

impl MovingAverage {
    fn new(window: Duration) -> Self {
        Self {
            window,
            data: VecDeque::new(),
        }
    }

    fn update(&mut self, value: f64, gpu_index: usize) {
        let now = Instant::now();

        // Remove old data points
        let cutoff = now - self.window;
        while let Some(front) = self.data.front() {
            if front.timestamp < cutoff {
                self.data.pop_front();
            } else {
                break;
            }
        }

        // Add new data point
        self.data.push_back(HashrateDataPoint {
            timestamp: now,
            hashrate: value,
            gpu_index,
        });
    }

    fn get_average(&self) -> f64 {
        if self.data.is_empty() {
            return 0.0;
        }

        let sum: f64 = self.data.iter().map(|dp| dp.hashrate).sum();
        sum / self.data.len() as f64
    }

    fn get_average_by_gpu(&self, gpu_index: usize) -> f64 {
        let gpu_data: Vec<f64> = self.data
            .iter()
            .filter(|dp| dp.gpu_index == gpu_index)
            .map(|dp| dp.hashrate)
            .collect();

        if gpu_data.is_empty() {
            return 0.0;
        }

        gpu_data.iter().sum::<f64>() / gpu_data.len() as f64
    }
}

/// Hashrate measurement system
pub struct HashrateMeter {
    /// Moving averages for different time windows
    ma_5s: MovingAverage,
    ma_30s: MovingAverage,
    ma_5m: MovingAverage,
    ma_15m: MovingAverage,

    /// All historical data points
    history: Vec<HashrateDataPoint>,

    /// Current hashrate per GPU
    current_rates: HashMap<usize, f64>,

    /// Statistics
    total_hashes: u64,
    start_time: Instant,
    min_rate: f64,
    max_rate: f64,

    /// Share-based effective hashrate calculation
    shares_accepted: u64,
    shares_start_time: Instant,
    difficulty: f64,
}

impl HashrateMeter {
    /// Create a new hashrate meter
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            ma_5s: MovingAverage::new(Duration::from_secs(5)),
            ma_30s: MovingAverage::new(Duration::from_secs(30)),
            ma_5m: MovingAverage::new(Duration::from_secs(300)),
            ma_15m: MovingAverage::new(Duration::from_secs(900)),
            history: Vec::new(),
            current_rates: HashMap::new(),
            total_hashes: 0,
            start_time: now,
            min_rate: f64::MAX,
            max_rate: 0.0,
            shares_accepted: 0,
            shares_start_time: now,
            difficulty: 1.0,
        }
    }

    /// Update hashrate for a GPU
    pub fn update(&mut self, gpu_index: usize, hashrate: f64) {
        // Update moving averages
        self.ma_5s.update(hashrate, gpu_index);
        self.ma_30s.update(hashrate, gpu_index);
        self.ma_5m.update(hashrate, gpu_index);
        self.ma_15m.update(hashrate, gpu_index);

        // Update history
        self.history.push(HashrateDataPoint {
            timestamp: Instant::now(),
            hashrate,
            gpu_index,
        });

        // Update current rate
        self.current_rates.insert(gpu_index, hashrate);

        // Update min/max
        let total_rate = self.get_current_hashrate();
        if total_rate < self.min_rate {
            self.min_rate = total_rate;
        }
        if total_rate > self.max_rate {
            self.max_rate = total_rate;
        }
    }

    /// Update total hashes computed
    pub fn add_hashes(&mut self, hashes: u64) {
        self.total_hashes += hashes;
    }

    /// Record accepted share for effective hashrate
    pub fn record_share(&mut self, accepted: bool) {
        if accepted {
            self.shares_accepted += 1;
        }
    }

    /// Set current difficulty
    pub fn set_difficulty(&mut self, difficulty: f64) {
        self.difficulty = difficulty;
    }

    /// Get current total hashrate
    pub fn get_current_hashrate(&self) -> f64 {
        self.current_rates.values().sum()
    }

    /// Get hashrate for specific GPU
    pub fn get_gpu_hashrate(&self, gpu_index: usize) -> f64 {
        *self.current_rates.get(&gpu_index).unwrap_or(&0.0)
    }

    /// Calculate effective hashrate from shares
    fn calculate_effective_hashrate(&self) -> f64 {
        let elapsed = self.shares_start_time.elapsed().as_secs_f64();
        if elapsed < 1.0 {
            return 0.0;
        }

        // Effective hashrate = (shares * difficulty * 2^32) / time
        let hashes_per_share = self.difficulty * (2u64.pow(32) as f64);
        (self.shares_accepted as f64 * hashes_per_share) / elapsed
    }

    /// Get comprehensive statistics
    pub fn get_statistics(&self) -> HashrateStatistics {
        let current = self.get_current_hashrate();
        let effective = self.calculate_effective_hashrate();

        // Calculate average
        let average = if !self.history.is_empty() {
            let sum: f64 = self.history.iter().map(|dp| dp.hashrate).sum();
            sum / self.history.len() as f64
        } else {
            0.0
        };

        // Calculate standard deviation
        let std_deviation = if !self.history.is_empty() && average > 0.0 {
            let variance: f64 = self.history
                .iter()
                .map(|dp| {
                    let diff = dp.hashrate - average;
                    diff * diff
                })
                .sum::<f64>() / self.history.len() as f64;
            variance.sqrt()
        } else {
            0.0
        };

        HashrateStatistics {
            current,
            avg_5s: self.ma_5s.get_average(),
            avg_30s: self.ma_30s.get_average(),
            avg_5m: self.ma_5m.get_average(),
            avg_15m: self.ma_15m.get_average(),
            min: if self.min_rate == f64::MAX { 0.0 } else { self.min_rate },
            max: self.max_rate,
            average,
            std_deviation,
            gpu_hashrates: self.current_rates.clone(),
            effective,
            efficiency_ratio: if current > 0.0 { effective / current } else { 0.0 },
        }
    }

    /// Get time-series data for a specific time range
    pub fn get_time_series(&self, duration: Duration) -> Vec<(Instant, f64)> {
        let cutoff = Instant::now() - duration;

        self.history
            .iter()
            .filter(|dp| dp.timestamp >= cutoff)
            .map(|dp| (dp.timestamp, dp.hashrate))
            .collect()
    }

    /// Reset all measurements
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

/// Hashrate sampler for periodic collection
pub struct HashrateSampler {
    /// Sampling interval
    interval: Duration,

    /// Collected samples
    samples: Vec<(Instant, f64)>,

    /// Maximum samples to retain
    max_samples: usize,
}

impl HashrateSampler {
    /// Create a new sampler
    pub fn new(interval: Duration, max_samples: usize) -> Self {
        Self {
            interval,
            samples: Vec::with_capacity(max_samples),
            max_samples,
        }
    }

    /// Add a sample
    pub fn add_sample(&mut self, hashrate: f64) {
        let now = Instant::now();

        // Check if enough time has passed since last sample
        if let Some((last_time, _)) = self.samples.last() {
            if now.duration_since(*last_time) < self.interval {
                return;
            }
        }

        self.samples.push((now, hashrate));

        // Limit samples
        if self.samples.len() > self.max_samples {
            self.samples.remove(0);
        }
    }

    /// Get statistical analysis of samples
    pub fn analyze(&self) -> Option<SampleAnalysis> {
        if self.samples.is_empty() {
            return None;
        }

        let values: Vec<f64> = self.samples.iter().map(|(_, v)| *v).collect();

        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let sum: f64 = values.iter().sum();
        let mean = sum / values.len() as f64;

        // Calculate standard deviation
        let variance: f64 = values
            .iter()
            .map(|v| {
                let diff = v - mean;
                diff * diff
            })
            .sum::<f64>() / values.len() as f64;
        let std_dev = variance.sqrt();

        // Calculate median
        let mut sorted = values.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = if sorted.len() % 2 == 0 {
            (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
        } else {
            sorted[sorted.len() / 2]
        };

        Some(SampleAnalysis {
            min,
            max,
            mean,
            median,
            std_deviation: std_dev,
            sample_count: values.len(),
        })
    }
}

/// Statistical analysis of samples
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleAnalysis {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub median: f64,
    pub std_deviation: f64,
    pub sample_count: usize,
}