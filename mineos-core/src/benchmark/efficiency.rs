/// Power efficiency calculation and metrics
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Power consumption metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerMetrics {
    /// Total system power (W)
    pub total_power: f32,

    /// Per-GPU power consumption
    pub gpu_power: HashMap<usize, f32>,

    /// Average temperature across GPUs
    pub avg_temperature: f32,

    /// Per-GPU temperatures
    pub gpu_temperatures: HashMap<usize, f32>,

    /// Power efficiency (H/W)
    pub efficiency_hw: f64,

    /// Energy efficiency (MH/J)
    pub efficiency_mhj: f64,

    /// Cost efficiency (H/$) based on electricity cost
    pub cost_efficiency: Option<f64>,

    /// Power state distribution
    pub power_states: PowerStateDistribution,
}

/// Power state distribution tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerStateDistribution {
    /// Time in P0 state (maximum performance)
    pub p0_time_percent: f32,

    /// Time in P2 state (normal)
    pub p2_time_percent: f32,

    /// Time in P8 state (idle)
    pub p8_time_percent: f32,

    /// Average power state
    pub avg_pstate: f32,
}

/// Efficiency calculator
pub struct EfficiencyCalculator {
    /// Current power readings
    current_power: HashMap<usize, f32>,

    /// Historical power data
    power_history: Vec<PowerSample>,

    /// Electricity cost per kWh
    electricity_cost: Option<f64>,

    /// Power state tracker
    pstate_tracker: PowerStateTracker,
}

/// Power sample data point
#[derive(Debug, Clone)]
struct PowerSample {
    timestamp: std::time::Instant,
    gpu_index: usize,
    power_watts: f32,
    temperature: f32,
    pstate: u32,
}

/// Power state tracker
struct PowerStateTracker {
    state_times: HashMap<u32, std::time::Duration>,
    last_state: u32,
    last_update: std::time::Instant,
}

impl PowerStateTracker {
    fn new() -> Self {
        Self {
            state_times: HashMap::new(),
            last_state: 2, // P2 is typical running state
            last_update: std::time::Instant::now(),
        }
    }

    fn update_state(&mut self, new_state: u32) {
        let now = std::time::Instant::now();
        let duration = now - self.last_update;

        *self.state_times.entry(self.last_state).or_insert(std::time::Duration::ZERO) += duration;

        self.last_state = new_state;
        self.last_update = now;
    }

    fn get_distribution(&self) -> PowerStateDistribution {
        let total_time: std::time::Duration = self.state_times.values().sum();

        if total_time.as_secs() == 0 {
            return PowerStateDistribution {
                p0_time_percent: 0.0,
                p2_time_percent: 100.0,
                p8_time_percent: 0.0,
                avg_pstate: 2.0,
            };
        }

        let total_secs = total_time.as_secs_f32();

        let p0_time = self.state_times.get(&0).copied().unwrap_or(std::time::Duration::ZERO);
        let p2_time = self.state_times.get(&2).copied().unwrap_or(std::time::Duration::ZERO);
        let p8_time = self.state_times.get(&8).copied().unwrap_or(std::time::Duration::ZERO);

        // Calculate weighted average P-state
        let mut weighted_sum = 0.0;
        for (state, duration) in &self.state_times {
            weighted_sum += *state as f32 * duration.as_secs_f32();
        }
        let avg_pstate = weighted_sum / total_secs;

        PowerStateDistribution {
            p0_time_percent: (p0_time.as_secs_f32() / total_secs) * 100.0,
            p2_time_percent: (p2_time.as_secs_f32() / total_secs) * 100.0,
            p8_time_percent: (p8_time.as_secs_f32() / total_secs) * 100.0,
            avg_pstate,
        }
    }
}

impl EfficiencyCalculator {
    /// Create a new efficiency calculator
    pub fn new() -> Self {
        Self {
            current_power: HashMap::new(),
            power_history: Vec::new(),
            electricity_cost: None,
            pstate_tracker: PowerStateTracker::new(),
        }
    }

    /// Set electricity cost per kWh
    pub fn set_electricity_cost(&mut self, cost_per_kwh: f64) {
        self.electricity_cost = Some(cost_per_kwh);
    }

    /// Update power reading for a GPU
    pub fn update_power(&mut self, gpu_index: usize, power_watts: f32, temperature: f32, pstate: u32) {
        self.current_power.insert(gpu_index, power_watts);

        self.power_history.push(PowerSample {
            timestamp: std::time::Instant::now(),
            gpu_index,
            power_watts,
            temperature,
            pstate,
        });

        // Update power state tracking
        self.pstate_tracker.update_state(pstate);

        // Limit history size
        const MAX_HISTORY: usize = 10000;
        if self.power_history.len() > MAX_HISTORY {
            self.power_history.drain(0..self.power_history.len() - MAX_HISTORY);
        }
    }

    /// Calculate efficiency metrics
    pub fn calculate_efficiency(&self, hashrate: f64) -> (f64, f64) {
        let total_power = self.get_total_power();

        if total_power <= 0.0 {
            return (0.0, 0.0);
        }

        // Hash per Watt
        let efficiency_hw = hashrate / total_power as f64;

        // MegaHash per Joule (1W = 1J/s)
        let efficiency_mhj = (hashrate / 1_000_000.0) / total_power as f64;

        (efficiency_hw, efficiency_mhj)
    }

    /// Calculate cost efficiency
    pub fn calculate_cost_efficiency(&self, hashrate: f64) -> Option<f64> {
        let cost_per_kwh = self.electricity_cost?;
        let total_power = self.get_total_power();

        if total_power <= 0.0 {
            return None;
        }

        // Power in kW
        let power_kw = total_power / 1000.0;

        // Cost per hour
        let cost_per_hour = power_kw as f64 * cost_per_kwh;

        // Hashes per dollar
        let hashes_per_hour = hashrate * 3600.0;
        let efficiency = hashes_per_hour / cost_per_hour;

        Some(efficiency)
    }

    /// Get total power consumption
    pub fn get_total_power(&self) -> f32 {
        self.current_power.values().sum()
    }

    /// Get average temperature
    pub fn get_avg_temperature(&self) -> f32 {
        if self.power_history.is_empty() {
            return 0.0;
        }

        // Get recent samples (last 60 seconds)
        let cutoff = std::time::Instant::now() - std::time::Duration::from_secs(60);
        let recent_samples: Vec<&PowerSample> = self.power_history
            .iter()
            .filter(|s| s.timestamp > cutoff)
            .collect();

        if recent_samples.is_empty() {
            return 0.0;
        }

        let sum: f32 = recent_samples.iter().map(|s| s.temperature).sum();
        sum / recent_samples.len() as f32
    }

    /// Get comprehensive metrics
    pub fn get_metrics(&self) -> PowerMetrics {
        let total_power = self.get_total_power();
        let avg_temperature = self.get_avg_temperature();

        // Placeholder for efficiency calculations (need hashrate)
        let (efficiency_hw, efficiency_mhj) = (0.0, 0.0);

        // Build per-GPU temperature map
        let mut gpu_temperatures = HashMap::new();
        for sample in self.power_history.iter().rev().take(100) {
            gpu_temperatures.entry(sample.gpu_index)
                .or_insert(sample.temperature);
        }

        PowerMetrics {
            total_power,
            gpu_power: self.current_power.clone(),
            avg_temperature,
            gpu_temperatures,
            efficiency_hw,
            efficiency_mhj,
            cost_efficiency: None,
            power_states: self.pstate_tracker.get_distribution(),
        }
    }

    /// Get power trend analysis
    pub fn analyze_power_trend(&self, duration: std::time::Duration) -> PowerTrend {
        let cutoff = std::time::Instant::now() - duration;
        let samples: Vec<&PowerSample> = self.power_history
            .iter()
            .filter(|s| s.timestamp > cutoff)
            .collect();

        if samples.len() < 2 {
            return PowerTrend::default();
        }

        // Calculate average power over time windows
        let window_size = samples.len() / 10; // 10 windows
        if window_size == 0 {
            return PowerTrend::default();
        }

        let mut windows = Vec::new();
        for chunk in samples.chunks(window_size) {
            let avg_power: f32 = chunk.iter().map(|s| s.power_watts).sum::<f32>() / chunk.len() as f32;
            windows.push(avg_power);
        }

        // Calculate trend
        let first_half_avg: f32 = windows[..windows.len()/2].iter().sum::<f32>() / (windows.len()/2) as f32;
        let second_half_avg: f32 = windows[windows.len()/2..].iter().sum::<f32>() / (windows.len() - windows.len()/2) as f32;

        let trend = if second_half_avg > first_half_avg * 1.05 {
            TrendDirection::Increasing
        } else if second_half_avg < first_half_avg * 0.95 {
            TrendDirection::Decreasing
        } else {
            TrendDirection::Stable
        };

        PowerTrend {
            direction: trend,
            change_percent: ((second_half_avg - first_half_avg) / first_half_avg) * 100.0,
            avg_power: windows.iter().sum::<f32>() / windows.len() as f32,
            min_power: windows.iter().cloned().fold(f32::INFINITY, f32::min),
            max_power: windows.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
        }
    }
}

/// Power consumption trend analysis
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PowerTrend {
    pub direction: TrendDirection,
    pub change_percent: f32,
    pub avg_power: f32,
    pub min_power: f32,
    pub max_power: f32,
}

/// Trend direction
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum TrendDirection {
    Increasing,
    #[default]
    Stable,
    Decreasing,
}