/// Temperature impact analysis
use std::collections::{HashMap, VecDeque};
use serde::{Deserialize, Serialize};

/// Thermal monitoring data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalData {
    /// Current core temperatures
    pub core_temps: HashMap<usize, f32>,

    /// Current memory temperatures
    pub memory_temps: HashMap<usize, f32>,

    /// Temperature trends
    pub temp_trends: HashMap<usize, TempTrend>,

    /// Thermal throttling events
    pub throttle_events: Vec<ThrottleEvent>,

    /// Performance impact analysis
    pub impact_analysis: ThermalImpact,

    /// Optimal temperature range
    pub optimal_range: TempRange,

    /// Cooling efficiency score
    pub cooling_efficiency: f64,
}

/// Temperature trend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempTrend {
    pub current: f32,
    pub avg_5min: f32,
    pub avg_15min: f32,
    pub min: f32,
    pub max: f32,
    pub trend: TrendDirection,
}

/// Throttling event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThrottleEvent {
    pub timestamp: std::time::SystemTime,
    pub gpu_index: usize,
    pub temperature: f32,
    pub hashrate_impact: f64,
    pub duration_ms: u64,
}

/// Thermal impact on performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalImpact {
    /// Hashrate degradation per degree
    pub degradation_per_degree: f64,

    /// Temperature at which throttling starts
    pub throttle_threshold: f32,

    /// Estimated performance loss due to temperature
    pub performance_loss_percent: f64,

    /// Correlation coefficient between temp and hashrate
    pub temp_hashrate_correlation: f64,
}

/// Temperature range
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempRange {
    pub min: f32,
    pub max: f32,
    pub optimal: f32,
}

/// Trend direction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrendDirection {
    Rising,
    Stable,
    Falling,
}

/// Temperature sample
#[derive(Debug, Clone)]
struct TempSample {
    timestamp: std::time::Instant,
    gpu_index: usize,
    core_temp: f32,
    memory_temp: Option<f32>,
    hashrate: f64,
}

/// Thermal monitor
pub struct ThermalMonitor {
    /// Temperature history
    temp_history: VecDeque<TempSample>,

    /// Maximum history size
    max_history: usize,

    /// Throttle event tracker
    throttle_tracker: ThrottleTracker,

    /// Temperature statistics
    temp_stats: HashMap<usize, TempStats>,

    /// Current temperatures
    current_temps: HashMap<usize, (f32, Option<f32>)>,
}

/// Temperature statistics
struct TempStats {
    samples: VecDeque<f32>,
    min: f32,
    max: f32,
    sum: f32,
}

impl TempStats {
    fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            min: f32::MAX,
            max: f32::MIN,
            sum: 0.0,
        }
    }

    fn add_sample(&mut self, temp: f32) {
        self.samples.push_back(temp);
        if self.samples.len() > 1000 {
            if let Some(old) = self.samples.pop_front() {
                self.sum -= old;
            }
        }
        self.sum += temp;
        self.min = self.min.min(temp);
        self.max = self.max.max(temp);
    }

    fn average(&self) -> f32 {
        if self.samples.is_empty() {
            0.0
        } else {
            self.sum / self.samples.len() as f32
        }
    }
}

/// Throttle event tracker
struct ThrottleTracker {
    active_throttles: HashMap<usize, ThrottleState>,
    completed_events: Vec<ThrottleEvent>,
}

struct ThrottleState {
    start_time: std::time::Instant,
    start_temp: f32,
    baseline_hashrate: f64,
}

impl ThrottleTracker {
    fn new() -> Self {
        Self {
            active_throttles: HashMap::new(),
            completed_events: Vec::new(),
        }
    }

    fn start_throttle(&mut self, gpu_index: usize, temp: f32, baseline_hashrate: f64) {
        self.active_throttles.insert(gpu_index, ThrottleState {
            start_time: std::time::Instant::now(),
            start_temp: temp,
            baseline_hashrate,
        });
    }

    fn end_throttle(&mut self, gpu_index: usize, current_hashrate: f64) {
        if let Some(state) = self.active_throttles.remove(&gpu_index) {
            let duration = state.start_time.elapsed();
            let hashrate_impact = if state.baseline_hashrate > 0.0 {
                ((state.baseline_hashrate - current_hashrate) / state.baseline_hashrate) * 100.0
            } else {
                0.0
            };

            self.completed_events.push(ThrottleEvent {
                timestamp: std::time::SystemTime::now(),
                gpu_index,
                temperature: state.start_temp,
                hashrate_impact,
                duration_ms: duration.as_millis() as u64,
            });
        }
    }
}

impl ThermalMonitor {
    /// Create a new thermal monitor
    pub fn new() -> Self {
        Self {
            temp_history: VecDeque::with_capacity(10000),
            max_history: 10000,
            throttle_tracker: ThrottleTracker::new(),
            temp_stats: HashMap::new(),
            current_temps: HashMap::new(),
        }
    }

    /// Update temperature reading
    pub fn update_temperature(
        &mut self,
        gpu_index: usize,
        core_temp: f32,
        memory_temp: Option<f32>,
        hashrate: f64,
    ) {
        // Add to history
        let sample = TempSample {
            timestamp: std::time::Instant::now(),
            gpu_index,
            core_temp,
            memory_temp,
            hashrate,
        };

        self.temp_history.push_back(sample);
        if self.temp_history.len() > self.max_history {
            self.temp_history.pop_front();
        }

        // Update statistics
        self.temp_stats
            .entry(gpu_index)
            .or_insert_with(TempStats::new)
            .add_sample(core_temp);

        // Update current
        self.current_temps.insert(gpu_index, (core_temp, memory_temp));

        // Check for throttling
        self.detect_throttling(gpu_index, core_temp, hashrate);
    }

    /// Detect thermal throttling
    fn detect_throttling(&mut self, gpu_index: usize, temp: f32, hashrate: f64) {
        const THROTTLE_TEMP: f32 = 83.0; // Typical GPU throttle temperature

        if temp >= THROTTLE_TEMP {
            if !self.throttle_tracker.active_throttles.contains_key(&gpu_index) {
                // Get baseline hashrate from history
                let baseline = self.get_baseline_hashrate(gpu_index);
                self.throttle_tracker.start_throttle(gpu_index, temp, baseline);
            }
        } else if self.throttle_tracker.active_throttles.contains_key(&gpu_index) {
            self.throttle_tracker.end_throttle(gpu_index, hashrate);
        }
    }

    /// Get baseline hashrate (before throttling)
    fn get_baseline_hashrate(&self, gpu_index: usize) -> f64 {
        // Look for hashrate when temperature was lower
        let cutoff = std::time::Instant::now() - std::time::Duration::from_secs(300);

        self.temp_history
            .iter()
            .filter(|s| s.gpu_index == gpu_index && s.timestamp > cutoff && s.core_temp < 80.0)
            .map(|s| s.hashrate)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0)
    }

    /// Analyze temperature impact on performance
    pub fn analyze_impact(&self) -> ThermalImpact {
        // Collect paired temperature-hashrate data
        let mut data_points: Vec<(f32, f64)> = Vec::new();

        for sample in &self.temp_history {
            data_points.push((sample.core_temp, sample.hashrate));
        }

        if data_points.len() < 10 {
            return ThermalImpact {
                degradation_per_degree: 0.0,
                throttle_threshold: 83.0,
                performance_loss_percent: 0.0,
                temp_hashrate_correlation: 0.0,
            };
        }

        // Calculate correlation and degradation
        let correlation = self.calculate_correlation(&data_points);
        let degradation = self.calculate_degradation(&data_points);
        let throttle_threshold = self.find_throttle_threshold(&data_points);
        let performance_loss = self.calculate_performance_loss(&data_points);

        ThermalImpact {
            degradation_per_degree: degradation,
            throttle_threshold,
            performance_loss_percent: performance_loss,
            temp_hashrate_correlation: correlation,
        }
    }

    /// Calculate correlation coefficient
    fn calculate_correlation(&self, data: &[(f32, f64)]) -> f64 {
        if data.len() < 2 {
            return 0.0;
        }

        let n = data.len() as f64;
        let sum_x: f64 = data.iter().map(|(x, _)| *x as f64).sum();
        let sum_y: f64 = data.iter().map(|(_, y)| *y).sum();
        let sum_xy: f64 = data.iter().map(|(x, y)| *x as f64 * y).sum();
        let sum_x2: f64 = data.iter().map(|(x, _)| (*x as f64).powi(2)).sum();
        let sum_y2: f64 = data.iter().map(|(_, y)| y.powi(2) as f64).sum();

        let numerator = n * sum_xy - sum_x * sum_y;
        let denominator = ((n * sum_x2 - sum_x.powi(2)) * (n * sum_y2 - sum_y.powi(2))).sqrt();

        if denominator == 0.0 {
            0.0
        } else {
            numerator / denominator
        }
    }

    /// Calculate performance degradation per degree
    fn calculate_degradation(&self, data: &[(f32, f64)]) -> f64 {
        // Simple linear regression to find slope
        if data.len() < 2 {
            return 0.0;
        }

        // Filter data to temperature range where degradation occurs (>70°C)
        let filtered: Vec<(f32, f64)> = data
            .iter()
            .filter(|(temp, _)| *temp > 70.0)
            .cloned()
            .collect();

        if filtered.len() < 2 {
            return 0.0;
        }

        // Calculate slope using least squares
        let n = filtered.len() as f64;
        let sum_x: f64 = filtered.iter().map(|(x, _)| *x as f64).sum();
        let sum_y: f64 = filtered.iter().map(|(_, y)| *y).sum();
        let sum_xy: f64 = filtered.iter().map(|(x, y)| *x as f64 * y).sum();
        let sum_x2: f64 = filtered.iter().map(|(x, _)| (*x as f64).powi(2)).sum();

        let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x.powi(2));

        slope.abs() // Return absolute value as degradation
    }

    /// Find temperature where throttling starts
    fn find_throttle_threshold(&self, data: &[(f32, f64)]) -> f32 {
        // Look for sharp drop in hashrate
        let mut sorted = data.to_vec();
        sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        for window in sorted.windows(2) {
            let temp_diff = window[1].0 - window[0].0;
            let hashrate_diff = window[1].1 - window[0].1;

            if temp_diff > 0.0 && hashrate_diff < -0.05 * window[0].1 {
                return window[0].0;
            }
        }

        83.0 // Default throttle temperature
    }

    /// Calculate overall performance loss
    fn calculate_performance_loss(&self, data: &[(f32, f64)]) -> f64 {
        if data.is_empty() {
            return 0.0;
        }

        // Find maximum hashrate (baseline)
        let max_hashrate = data.iter().map(|(_, h)| h).fold(0.0f64, |a, &b| a.max(b));
        if max_hashrate == 0.0 {
            return 0.0;
        }

        // Calculate average hashrate
        let avg_hashrate: f64 = data.iter().map(|(_, h)| h).sum::<f64>() / data.len() as f64;

        // Performance loss as percentage
        ((max_hashrate - avg_hashrate) / max_hashrate) * 100.0
    }

    /// Get current power consumption (placeholder)
    pub fn get_current_power(&self) -> f32 {
        // This would connect to actual power monitoring
        250.0
    }

    /// Get current temperature
    pub fn get_current_temperature(&self) -> f32 {
        if self.current_temps.is_empty() {
            return 0.0;
        }

        let sum: f32 = self.current_temps.values().map(|(core, _)| core).sum();
        sum / self.current_temps.len() as f32
    }

    /// Get comprehensive thermal data
    pub fn get_thermal_data(&self) -> ThermalData {
        let mut core_temps = HashMap::new();
        let mut memory_temps = HashMap::new();
        let mut temp_trends = HashMap::new();

        for (&gpu_index, &(core, mem)) in &self.current_temps {
            core_temps.insert(gpu_index, core);
            if let Some(m) = mem {
                memory_temps.insert(gpu_index, m);
            }

            // Calculate trend
            if let Some(stats) = self.temp_stats.get(&gpu_index) {
                let trend = TempTrend {
                    current: core,
                    avg_5min: stats.average(),
                    avg_15min: stats.average(), // Simplified
                    min: stats.min,
                    max: stats.max,
                    trend: TrendDirection::Stable, // Simplified
                };
                temp_trends.insert(gpu_index, trend);
            }
        }

        ThermalData {
            core_temps,
            memory_temps,
            temp_trends,
            throttle_events: self.throttle_tracker.completed_events.clone(),
            impact_analysis: self.analyze_impact(),
            optimal_range: TempRange {
                min: 60.0,
                max: 75.0,
                optimal: 68.0,
            },
            cooling_efficiency: self.calculate_cooling_efficiency(),
        }
    }

    /// Calculate cooling efficiency
    fn calculate_cooling_efficiency(&self) -> f64 {
        // Simplified: based on how well temperature is maintained
        let avg_temp = self.get_current_temperature();
        if avg_temp <= 0.0 {
            return 0.0;
        }

        // Ideal temperature around 65-70°C
        let ideal = 67.5f32;
        let deviation = (avg_temp - ideal).abs();

        // Score decreases with deviation
        let efficiency = (1.0 - (deviation / 20.0).min(1.0)) * 100.0;
        efficiency.max(0.0) as f64
    }
}