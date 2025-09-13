use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// GPU scheduling strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchedulingStrategy {
    /// Round-robin across all GPUs
    RoundRobin,

    /// Prefer least loaded GPU
    LeastLoaded,

    /// Based on GPU performance metrics
    PerformanceBased,

    /// Based on power efficiency
    PowerEfficient,

    /// Custom weighted scoring
    Weighted,
}

/// GPU load information
#[derive(Debug, Clone)]
pub struct GpuLoad {
    /// GPU index
    pub gpu_index: usize,

    /// Current utilization percentage
    pub utilization: f32,

    /// Memory usage percentage
    pub memory_usage: f32,

    /// Temperature in Celsius
    pub temperature: f32,

    /// Power usage in Watts
    pub power_watts: f32,

    /// Current hashrate
    pub hashrate: f64,

    /// Number of active work units
    pub active_work_units: usize,

    /// Last update time
    pub last_update: Instant,
}

impl Default for GpuLoad {
    fn default() -> Self {
        Self {
            gpu_index: 0,
            utilization: 0.0,
            memory_usage: 0.0,
            temperature: 0.0,
            power_watts: 0.0,
            hashrate: 0.0,
            active_work_units: 0,
            last_update: Instant::now(),
        }
    }
}

/// GPU capability information
#[derive(Debug, Clone)]
pub struct GpuCapability {
    /// GPU index
    pub gpu_index: usize,

    /// Maximum hashrate observed
    pub max_hashrate: f64,

    /// Memory size in MB
    pub memory_mb: usize,

    /// Compute capability
    pub compute_capability: (u32, u32),

    /// Maximum power limit
    pub max_power_watts: f32,

    /// Thermal limit
    pub thermal_limit: f32,

    /// Performance tier (0=lowest, higher=better)
    pub performance_tier: u32,
}

/// Configuration for GPU scheduler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuSchedulerConfig {
    /// Scheduling strategy to use
    pub strategy: SchedulingStrategy,

    /// Load balancing threshold (%)
    pub load_balance_threshold: f32,

    /// Temperature throttle threshold (Celsius)
    pub thermal_throttle: f32,

    /// Power limit per GPU (Watts)
    pub power_limit: Option<f32>,

    /// Enable adaptive scheduling
    pub adaptive_scheduling: bool,

    /// Rebalance interval
    pub rebalance_interval: Duration,

    /// Utilization target (%)
    pub target_utilization: f32,
}

impl Default for GpuSchedulerConfig {
    fn default() -> Self {
        Self {
            strategy: SchedulingStrategy::PerformanceBased,
            load_balance_threshold: 20.0, // 20% difference triggers rebalance
            thermal_throttle: 85.0,       // Throttle at 85°C
            power_limit: None,
            adaptive_scheduling: true,
            rebalance_interval: Duration::from_secs(30),
            target_utilization: 95.0,
        }
    }
}

/// Schedules work across multiple GPUs
pub struct GpuScheduler {
    /// Configuration
    config: GpuSchedulerConfig,

    /// GPU load information
    gpu_loads: RwLock<HashMap<usize, GpuLoad>>,

    /// GPU capabilities
    gpu_capabilities: RwLock<HashMap<usize, GpuCapability>>,

    /// Round-robin counter
    round_robin_counter: atomic::Atomic<usize>,

    /// Last rebalance time
    last_rebalance: RwLock<Instant>,

    /// Scheduling statistics
    stats: RwLock<SchedulingStats>,

    /// Number of GPUs
    num_gpus: usize,
}

/// Scheduling statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchedulingStats {
    pub total_scheduled: u64,
    pub rebalances_performed: u64,
    pub thermal_throttles: u64,
    pub power_throttles: u64,
    pub load_migrations: u64,
}

impl GpuScheduler {
    /// Create a new GPU scheduler
    pub fn new(config: GpuSchedulerConfig, num_gpus: usize) -> Self {
        let mut gpu_loads = HashMap::new();
        let mut gpu_capabilities = HashMap::new();

        // Initialize with default values
        for i in 0..num_gpus {
            gpu_loads.insert(i, GpuLoad {
                gpu_index: i,
                ..Default::default()
            });

            gpu_capabilities.insert(i, GpuCapability {
                gpu_index: i,
                max_hashrate: 100_000_000.0, // 100 MH/s default
                memory_mb: 8192,
                compute_capability: (7, 5),
                max_power_watts: 300.0,
                thermal_limit: 90.0,
                performance_tier: 1,
            });
        }

        Self {
            config,
            gpu_loads: RwLock::new(gpu_loads),
            gpu_capabilities: RwLock::new(gpu_capabilities),
            round_robin_counter: atomic::Atomic::new(0),
            last_rebalance: RwLock::new(Instant::now()),
            stats: RwLock::new(SchedulingStats::default()),
            num_gpus,
        }
    }

    /// Update GPU load information
    pub fn update_gpu_load(&self, load: GpuLoad) {
        // Store values before moving load
        let gpu_index = load.gpu_index;
        let temperature = load.temperature;
        let power_watts = load.power_watts;

        let mut loads = self.gpu_loads.write();
        loads.insert(gpu_index, load);

        // Check for thermal throttling
        if temperature > self.config.thermal_throttle {
            warn!("GPU {} thermal throttle: {}°C", gpu_index, temperature);
            self.stats.write().thermal_throttles += 1;
        }

        // Check for power throttling
        if let Some(limit) = self.config.power_limit {
            if power_watts > limit {
                warn!("GPU {} power throttle: {}W", gpu_index, power_watts);
                self.stats.write().power_throttles += 1;
            }
        }
    }

    /// Update GPU capability information
    pub fn update_gpu_capability(&self, capability: GpuCapability) {
        let mut capabilities = self.gpu_capabilities.write();
        capabilities.insert(capability.gpu_index, capability);
    }

    /// Select the best GPU for new work
    pub fn select_gpu(&self) -> Option<usize> {
        if self.num_gpus == 0 {
            return None;
        }

        let mut stats = self.stats.write();
        stats.total_scheduled += 1;

        // Check if rebalancing is needed
        if self.config.adaptive_scheduling {
            self.check_rebalance();
        }

        match self.config.strategy {
            SchedulingStrategy::RoundRobin => self.select_round_robin(),
            SchedulingStrategy::LeastLoaded => self.select_least_loaded(),
            SchedulingStrategy::PerformanceBased => self.select_performance_based(),
            SchedulingStrategy::PowerEfficient => self.select_power_efficient(),
            SchedulingStrategy::Weighted => self.select_weighted(),
        }
    }

    /// Round-robin selection
    fn select_round_robin(&self) -> Option<usize> {
        let index = self.round_robin_counter.fetch_add(1, atomic::Ordering::SeqCst);
        Some(index % self.num_gpus)
    }

    /// Select least loaded GPU
    fn select_least_loaded(&self) -> Option<usize> {
        let loads = self.gpu_loads.read();

        let mut best_gpu = None;
        let mut min_load = f32::MAX;

        for (&idx, load) in loads.iter() {
            // Skip overheated GPUs
            if load.temperature > self.config.thermal_throttle {
                continue;
            }

            // Calculate combined load score
            let load_score = load.utilization * 0.7 + load.memory_usage * 0.3;

            if load_score < min_load {
                min_load = load_score;
                best_gpu = Some(idx);
            }
        }

        best_gpu
    }

    /// Select based on performance metrics
    fn select_performance_based(&self) -> Option<usize> {
        let loads = self.gpu_loads.read();
        let capabilities = self.gpu_capabilities.read();

        let mut best_gpu = None;
        let mut best_score = f64::MIN;

        for (&idx, load) in loads.iter() {
            // Skip overheated GPUs
            if load.temperature > self.config.thermal_throttle {
                continue;
            }

            let capability = capabilities.get(&idx)?;

            // Calculate performance score
            let mut score = capability.max_hashrate;

            // Penalize based on current load
            score *= (100.0 - load.utilization as f64) / 100.0;

            // Penalize based on temperature
            let temp_factor = (self.config.thermal_throttle - load.temperature) / self.config.thermal_throttle;
            score *= temp_factor as f64;

            // Bonus for higher tier GPUs
            score *= 1.0 + (capability.performance_tier as f64 * 0.1);

            if score > best_score {
                best_score = score;
                best_gpu = Some(idx);
            }
        }

        best_gpu
    }

    /// Select most power-efficient GPU
    fn select_power_efficient(&self) -> Option<usize> {
        let loads = self.gpu_loads.read();
        let _capabilities = self.gpu_capabilities.read();

        let mut best_gpu = None;
        let mut best_efficiency = f64::MIN;

        for (&idx, load) in loads.iter() {
            if load.power_watts == 0.0 || load.hashrate == 0.0 {
                continue;
            }

            // Calculate efficiency (hashes per watt)
            let efficiency = load.hashrate / load.power_watts as f64;

            // Penalize overheated GPUs
            let temp_factor = if load.temperature < self.config.thermal_throttle {
                1.0
            } else {
                0.5
            };

            let adjusted_efficiency = efficiency * temp_factor;

            if adjusted_efficiency > best_efficiency {
                best_efficiency = adjusted_efficiency;
                best_gpu = Some(idx);
            }
        }

        best_gpu.or_else(|| self.select_least_loaded())
    }

    /// Select using weighted scoring
    fn select_weighted(&self) -> Option<usize> {
        let loads = self.gpu_loads.read();
        let capabilities = self.gpu_capabilities.read();

        let mut best_gpu = None;
        let mut best_score = f64::MIN;

        for (&idx, load) in loads.iter() {
            let capability = capabilities.get(&idx)?;

            // Weighted scoring factors
            let performance_weight = 0.4;
            let load_weight = 0.3;
            let thermal_weight = 0.2;
            let power_weight = 0.1;

            // Performance score (normalized)
            let perf_score = capability.max_hashrate / 1_000_000_000.0; // Normalize to GH/s

            // Load score (inverted, lower is better)
            let load_score = (100.0 - load.utilization) / 100.0;

            // Thermal score (normalized)
            let thermal_score = (self.config.thermal_throttle - load.temperature) / self.config.thermal_throttle;

            // Power score (efficiency)
            let power_score = if load.power_watts > 0.0 {
                (capability.max_power_watts - load.power_watts) / capability.max_power_watts
            } else {
                1.0
            };

            // Calculate weighted score
            let total_score = perf_score * performance_weight as f64
                + load_score as f64 * load_weight
                + thermal_score as f64 * thermal_weight
                + power_score as f64 * power_weight;

            if total_score > best_score {
                best_score = total_score;
                best_gpu = Some(idx);
            }
        }

        best_gpu
    }

    /// Check if rebalancing is needed
    fn check_rebalance(&self) {
        let now = Instant::now();
        let mut last_rebalance = self.last_rebalance.write();

        if now.duration_since(*last_rebalance) < self.config.rebalance_interval {
            return;
        }

        let loads = self.gpu_loads.read();

        // Calculate average utilization
        let total_util: f32 = loads.values().map(|l| l.utilization).sum();
        let avg_util = total_util / self.num_gpus as f32;

        // Find max and min utilization
        let max_util = loads.values().map(|l| l.utilization).fold(0.0f32, f32::max);
        let min_util = loads.values().map(|l| l.utilization).fold(100.0f32, f32::min);

        // Check if rebalancing is needed
        if max_util - min_util > self.config.load_balance_threshold {
            info!("Load imbalance detected: max={:.1}%, min={:.1}%, avg={:.1}%",
                  max_util, min_util, avg_util);

            let mut stats = self.stats.write();
            stats.rebalances_performed += 1;

            *last_rebalance = now;

            // In production, would trigger actual work migration here
        }
    }

    /// Get GPU loads
    pub fn get_gpu_loads(&self) -> Vec<GpuLoad> {
        let loads = self.gpu_loads.read();
        loads.values().cloned().collect()
    }

    /// Get scheduling statistics
    pub fn get_stats(&self) -> SchedulingStats {
        self.stats.read().clone()
    }

    /// Get recommended work size for a GPU
    pub fn get_recommended_work_size(&self, gpu_index: usize) -> u64 {
        let loads = self.gpu_loads.read();
        let capabilities = self.gpu_capabilities.read();

        let load = loads.get(&gpu_index);
        let capability = capabilities.get(&gpu_index);

        match (load, capability) {
            (Some(l), Some(c)) => {
                // Base size on hashrate and target completion time
                let target_seconds = 30.0; // Target 30 second work units
                let base_size = (c.max_hashrate * target_seconds) as u64;

                // Adjust based on current utilization
                let util_factor = (100.0 - l.utilization) / 100.0;
                let adjusted_size = (base_size as f32 * util_factor) as u64;

                // Clamp to reasonable bounds
                adjusted_size.max(10_000_000).min(1_000_000_000)
            }
            _ => 100_000_000, // Default 100M nonces
        }
    }
}