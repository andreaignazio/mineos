use std::sync::Arc;
use std::time::{Duration, Instant};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use mineos_stratum::protocol::MiningJob;
use mineos_hash::{Hash256, BlockHeader};

/// Work unit assigned to a GPU
#[derive(Debug, Clone)]
pub struct WorkUnit {
    /// Unique identifier for this work unit
    pub id: u64,

    /// Job ID from the pool
    pub job_id: String,

    /// Block header to mine
    pub header: BlockHeader,

    /// Target difficulty for this work
    pub target: Hash256,

    /// Starting nonce for this range
    pub nonce_start: u64,

    /// Number of nonces to search
    pub nonce_count: u64,

    /// GPU index assigned to this work
    pub gpu_index: usize,

    /// Creation timestamp
    pub created_at: Instant,

    /// Estimated completion time based on GPU hashrate
    pub estimated_duration: Duration,

    /// Clean job flag (should interrupt current work)
    pub clean: bool,
}

/// Result from a completed work unit
#[derive(Debug, Clone)]
pub struct WorkResult {
    /// Work unit ID this result is for
    pub work_id: u64,

    /// GPU that completed the work
    pub gpu_index: usize,

    /// Found nonce (if any)
    pub nonce: Option<u64>,

    /// Hash of the solution (if found)
    pub hash: Option<Hash256>,

    /// Mix hash for verification (algorithm specific)
    pub mix_hash: Option<Hash256>,

    /// Number of hashes computed
    pub hashes_computed: u64,

    /// Time taken to complete
    pub duration: Duration,

    /// Effective hashrate
    pub hashrate: f64,
}

/// Statistics for a GPU
#[derive(Debug, Clone, Default)]
pub struct GpuStats {
    /// Total work units completed
    pub units_completed: u64,

    /// Total hashes computed
    pub total_hashes: u64,

    /// Current hashrate (H/s)
    pub current_hashrate: f64,

    /// Average hashrate over time
    pub average_hashrate: f64,

    /// Last work completion time
    pub last_completion: Option<Instant>,

    /// Number of solutions found
    pub solutions_found: u64,

    /// Number of stale shares
    pub stale_shares: u64,
}

/// Configuration for work distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkDistributorConfig {
    /// Base work size in nonces (will be adjusted per GPU)
    pub base_work_size: u64,

    /// Minimum work size
    pub min_work_size: u64,

    /// Maximum work size
    pub max_work_size: u64,

    /// Number of work units to keep queued per GPU
    pub queue_depth: usize,

    /// Timeout for work completion
    pub work_timeout: Duration,

    /// Enable dynamic work sizing
    pub dynamic_sizing: bool,

    /// Work stealing threshold (% idle time)
    pub work_stealing_threshold: f32,
}

impl Default for WorkDistributorConfig {
    fn default() -> Self {
        Self {
            base_work_size: 100_000_000, // 100M nonces
            min_work_size: 10_000_000,   // 10M minimum
            max_work_size: 1_000_000_000, // 1B maximum
            queue_depth: 3,               // Keep 3 units queued
            work_timeout: Duration::from_secs(60),
            dynamic_sizing: true,
            work_stealing_threshold: 0.1, // 10% idle time triggers stealing
        }
    }
}

/// Manages work distribution across GPUs
pub struct WorkDistributor {
    /// Configuration
    config: WorkDistributorConfig,

    /// Current mining job
    current_job: RwLock<Option<Arc<MiningJob>>>,

    /// Next work unit ID
    next_work_id: atomic::Atomic<u64>,

    /// Active work units by ID
    active_work: DashMap<u64, WorkUnit>,

    /// GPU statistics
    gpu_stats: DashMap<usize, GpuStats>,

    /// Work queues per GPU
    work_queues: DashMap<usize, Vec<WorkUnit>>,

    /// Number of GPUs
    num_gpus: usize,

    /// Global nonce offset for current job
    global_nonce_offset: atomic::Atomic<u64>,
}

impl WorkDistributor {
    /// Create a new work distributor
    pub fn new(config: WorkDistributorConfig, num_gpus: usize) -> Self {
        // Initialize per-GPU structures
        let gpu_stats = DashMap::new();
        let work_queues = DashMap::new();

        for gpu_idx in 0..num_gpus {
            gpu_stats.insert(gpu_idx, GpuStats::default());
            work_queues.insert(gpu_idx, Vec::new());
        }

        Self {
            config,
            current_job: RwLock::new(None),
            next_work_id: atomic::Atomic::new(0),
            active_work: DashMap::new(),
            gpu_stats,
            work_queues,
            num_gpus,
            global_nonce_offset: atomic::Atomic::new(0),
        }
    }

    /// Update the current mining job
    pub fn update_job(&self, job: MiningJob, header: BlockHeader, target: Hash256) {
        info!("Updating mining job: {}", job.job_id);

        // Clear existing work if this is a clean job
        if job.clean_jobs {
            self.clear_all_work();
        }

        // Update current job
        *self.current_job.write() = Some(Arc::new(job));

        // Reset nonce offset
        self.global_nonce_offset.store(0, atomic::Ordering::SeqCst);

        // Generate initial work for all GPUs
        for gpu_idx in 0..self.num_gpus {
            self.generate_work_for_gpu(gpu_idx, &header, &target);
        }
    }

    /// Get next work unit for a GPU
    pub fn get_work(&self, gpu_index: usize) -> Option<WorkUnit> {
        // Check queue first
        if let Some(mut queue) = self.work_queues.get_mut(&gpu_index) {
            if let Some(work) = queue.pop() {
                // Track as active
                self.active_work.insert(work.id, work.clone());

                // Generate replacement work
                if queue.len() < self.config.queue_depth {
                    if let Some(job) = self.current_job.read().as_ref() {
                        // Reconstruct header and target (simplified for now)
                        // In production, these would be cached
                        let header = BlockHeader::default();
                        let target = Hash256::default();
                        self.generate_work_for_gpu(gpu_index, &header, &target);
                    }
                }

                return Some(work);
            }
        }

        // Try work stealing if enabled
        if self.config.work_stealing_threshold > 0.0 {
            self.steal_work(gpu_index)
        } else {
            None
        }
    }

    /// Submit a work result
    pub fn submit_result(&self, result: WorkResult) {
        debug!("GPU {} completed work {} in {:?}",
               result.gpu_index, result.work_id, result.duration);

        // Remove from active work
        self.active_work.remove(&result.work_id);

        // Update GPU statistics
        if let Some(mut stats) = self.gpu_stats.get_mut(&result.gpu_index) {
            stats.units_completed += 1;
            stats.total_hashes += result.hashes_computed;
            stats.current_hashrate = result.hashrate;

            // Update average hashrate
            let alpha = 0.1; // Exponential moving average factor
            stats.average_hashrate = (1.0 - alpha) * stats.average_hashrate + alpha * result.hashrate;

            stats.last_completion = Some(Instant::now());

            if result.nonce.is_some() {
                stats.solutions_found += 1;
            }
        }

        // Generate more work for this GPU
        if let Some(job) = self.current_job.read().as_ref() {
            let header = BlockHeader::default();
            let target = Hash256::default();
            self.generate_work_for_gpu(result.gpu_index, &header, &target);
        }
    }

    /// Generate work for a specific GPU
    fn generate_work_for_gpu(&self, gpu_index: usize, header: &BlockHeader, target: &Hash256) {
        let job = match self.current_job.read().as_ref() {
            Some(j) => j.clone(),
            None => return,
        };

        // Calculate work size based on GPU performance
        let work_size = self.calculate_work_size(gpu_index);

        // Get next nonce range
        let nonce_start = self.global_nonce_offset.fetch_add(work_size, atomic::Ordering::SeqCst);

        // Create work unit
        let work = WorkUnit {
            id: self.next_work_id.fetch_add(1, atomic::Ordering::SeqCst),
            job_id: job.job_id.clone(),
            header: header.clone(),
            target: target.clone(),
            nonce_start,
            nonce_count: work_size,
            gpu_index,
            created_at: Instant::now(),
            estimated_duration: self.estimate_duration(gpu_index, work_size),
            clean: false,
        };

        // Add to queue
        if let Some(mut queue) = self.work_queues.get_mut(&gpu_index) {
            if queue.len() < self.config.queue_depth * 2 {
                queue.push(work);
            }
        }
    }

    /// Calculate dynamic work size for a GPU
    fn calculate_work_size(&self, gpu_index: usize) -> u64 {
        if !self.config.dynamic_sizing {
            return self.config.base_work_size;
        }

        // Get GPU stats
        let stats = self.gpu_stats.get(&gpu_index);
        let avg_hashrate = stats.as_ref()
            .map(|s| s.average_hashrate)
            .unwrap_or(100_000_000.0); // Default 100 MH/s

        // Calculate total hashrate
        let total_hashrate: f64 = self.gpu_stats.iter()
            .map(|entry| entry.value().average_hashrate)
            .sum();

        if total_hashrate == 0.0 {
            return self.config.base_work_size;
        }

        // Scale work size based on GPU's share of total hashrate
        let gpu_share = avg_hashrate / total_hashrate;
        let scaled_size = (self.config.base_work_size as f64 * gpu_share * self.num_gpus as f64) as u64;

        // Apply bounds
        scaled_size.max(self.config.min_work_size)
                   .min(self.config.max_work_size)
    }

    /// Estimate work duration for a GPU
    fn estimate_duration(&self, gpu_index: usize, work_size: u64) -> Duration {
        let stats = self.gpu_stats.get(&gpu_index);
        let hashrate = stats.as_ref()
            .map(|s| s.average_hashrate)
            .unwrap_or(100_000_000.0);

        if hashrate > 0.0 {
            let seconds = work_size as f64 / hashrate;
            Duration::from_secs_f64(seconds)
        } else {
            Duration::from_secs(30) // Default estimate
        }
    }

    /// Steal work from another GPU's queue
    fn steal_work(&self, gpu_index: usize) -> Option<WorkUnit> {
        // Find GPU with most queued work
        let mut max_queue_gpu = None;
        let mut max_queue_size = 0;

        for entry in self.work_queues.iter() {
            let (idx, queue) = entry.pair();
            if *idx != gpu_index && queue.len() > max_queue_size {
                max_queue_gpu = Some(*idx);
                max_queue_size = queue.len();
            }
        }

        // Steal from the GPU with most work
        if let Some(victim_gpu) = max_queue_gpu {
            if max_queue_size > 1 {
                if let Some(mut queue) = self.work_queues.get_mut(&victim_gpu) {
                    if let Some(mut work) = queue.pop() {
                        debug!("GPU {} stealing work from GPU {}", gpu_index, victim_gpu);
                        work.gpu_index = gpu_index;
                        return Some(work);
                    }
                }
            }
        }

        None
    }

    /// Clear all active work (for clean jobs)
    fn clear_all_work(&self) {
        info!("Clearing all active work for clean job");

        self.active_work.clear();

        for mut queue in self.work_queues.iter_mut() {
            queue.clear();
        }
    }

    /// Get current statistics for all GPUs
    pub fn get_stats(&self) -> Vec<(usize, GpuStats)> {
        self.gpu_stats.iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect()
    }

    /// Get total hashrate across all GPUs
    pub fn get_total_hashrate(&self) -> f64 {
        self.gpu_stats.iter()
            .map(|entry| entry.value().current_hashrate)
            .sum()
    }

    /// Check for timed out work
    pub fn check_timeouts(&self) -> Vec<WorkUnit> {
        let now = Instant::now();
        let mut timed_out = Vec::new();

        for entry in self.active_work.iter() {
            let work = entry.value();
            if now.duration_since(work.created_at) > self.config.work_timeout {
                warn!("Work {} timed out on GPU {}", work.id, work.gpu_index);
                timed_out.push(work.clone());
            }
        }

        // Remove timed out work
        for work in &timed_out {
            self.active_work.remove(&work.id);

            // Mark GPU as potentially having issues
            if let Some(mut stats) = self.gpu_stats.get_mut(&work.gpu_index) {
                stats.stale_shares += 1;
            }
        }

        timed_out
    }
}