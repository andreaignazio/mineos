use std::collections::{HashMap, VecDeque};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// A range of nonces assigned to a GPU
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonceRange {
    /// Starting nonce (inclusive)
    pub start: u64,

    /// Ending nonce (exclusive)
    pub end: u64,

    /// GPU this range is assigned to
    pub gpu_index: usize,

    /// Job ID this range is for
    pub job_id: String,
}

impl NonceRange {
    /// Create a new nonce range
    pub fn new(start: u64, size: u64, gpu_index: usize, job_id: String) -> Self {
        Self {
            start,
            end: start + size,
            gpu_index,
            job_id,
        }
    }

    /// Get the size of this range
    pub fn size(&self) -> u64 {
        self.end - self.start
    }

    /// Check if a nonce is within this range
    pub fn contains(&self, nonce: u64) -> bool {
        nonce >= self.start && nonce < self.end
    }

    /// Split this range into two parts
    pub fn split(&self, at: u64) -> Option<(Self, Self)> {
        if at <= self.start || at >= self.end {
            return None;
        }

        let left = NonceRange {
            start: self.start,
            end: at,
            gpu_index: self.gpu_index,
            job_id: self.job_id.clone(),
        };

        let right = NonceRange {
            start: at,
            end: self.end,
            gpu_index: self.gpu_index,
            job_id: self.job_id.clone(),
        };

        Some((left, right))
    }
}

/// Configuration for nonce management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NonceManagerConfig {
    /// Starting nonce for new jobs
    pub initial_nonce: u64,

    /// Maximum nonce value (2^64 - 1 for full range)
    pub max_nonce: u64,

    /// Default range size per allocation
    pub default_range_size: u64,

    /// Enable range recycling for failed GPUs
    pub enable_recycling: bool,

    /// Maximum ranges to track per job
    pub max_ranges_per_job: usize,
}

impl Default for NonceManagerConfig {
    fn default() -> Self {
        Self {
            initial_nonce: 0,
            max_nonce: u64::MAX,
            default_range_size: 100_000_000, // 100M nonces
            enable_recycling: true,
            max_ranges_per_job: 10000,
        }
    }
}

/// Manages nonce range allocation for GPUs
pub struct NonceManager {
    /// Configuration
    config: NonceManagerConfig,

    /// Current nonce offset per job
    job_offsets: RwLock<HashMap<String, u64>>,

    /// Active ranges per job
    active_ranges: RwLock<HashMap<String, Vec<NonceRange>>>,

    /// Recycled ranges available for reallocation
    recycled_ranges: RwLock<HashMap<String, VecDeque<NonceRange>>>,

    /// Statistics
    stats: RwLock<NonceStats>,
}

/// Nonce management statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NonceStats {
    pub total_ranges_allocated: u64,
    pub total_nonces_allocated: u64,
    pub ranges_recycled: u64,
    pub nonces_recycled: u64,
    pub active_jobs: usize,
    pub active_ranges: usize,
    pub collisions_prevented: u64,
}

impl NonceManager {
    /// Create a new nonce manager
    pub fn new(config: NonceManagerConfig) -> Self {
        Self {
            config,
            job_offsets: RwLock::new(HashMap::new()),
            active_ranges: RwLock::new(HashMap::new()),
            recycled_ranges: RwLock::new(HashMap::new()),
            stats: RwLock::new(NonceStats::default()),
        }
    }

    /// Allocate a nonce range for a GPU
    pub fn allocate_range(
        &self,
        job_id: &str,
        gpu_index: usize,
        requested_size: Option<u64>,
    ) -> Option<NonceRange> {
        let size = requested_size.unwrap_or(self.config.default_range_size);

        // First, try to get a recycled range
        if self.config.enable_recycling {
            if let Some(range) = self.get_recycled_range(job_id, gpu_index, size) {
                debug!("Allocated recycled range for GPU {}: {:?}", gpu_index, range);
                return Some(range);
            }
        }

        // Allocate new range
        let mut offsets = self.job_offsets.write();
        let offset = offsets.entry(job_id.to_string()).or_insert(self.config.initial_nonce);

        // Check if we have space
        if *offset + size > self.config.max_nonce {
            warn!("Nonce space exhausted for job {}", job_id);
            return None;
        }

        // Create range
        let range = NonceRange::new(*offset, size, gpu_index, job_id.to_string());

        // Update offset
        *offset += size;

        // Track active range
        let mut active = self.active_ranges.write();
        let job_ranges = active.entry(job_id.to_string()).or_insert_with(Vec::new);

        // Limit number of ranges per job
        if job_ranges.len() >= self.config.max_ranges_per_job {
            warn!("Maximum ranges reached for job {}", job_id);
            return None;
        }

        let range_start = range.start;
        job_ranges.push(range.clone());

        // Update statistics
        let mut stats = self.stats.write();
        stats.total_ranges_allocated += 1;
        stats.total_nonces_allocated += size;
        stats.active_jobs = offsets.len();
        stats.active_ranges = active.values().map(|v| v.len()).sum();

        info!("Allocated range for GPU {}: start={}, size={}", gpu_index, range_start, size);

        Some(range)
    }

    /// Mark a range as completed (can be recycled)
    pub fn complete_range(&self, job_id: &str, range: NonceRange) {
        // Remove from active ranges
        let mut active = self.active_ranges.write();
        if let Some(job_ranges) = active.get_mut(job_id) {
            job_ranges.retain(|r| *r != range);
        }

        // Add to recycled if enabled
        if self.config.enable_recycling {
            let range_size = range.size();
            let mut recycled = self.recycled_ranges.write();
            let job_recycled = recycled.entry(job_id.to_string()).or_insert_with(VecDeque::new);
            job_recycled.push_back(range.clone());

            let mut stats = self.stats.write();
            stats.ranges_recycled += 1;
            stats.nonces_recycled += range_size;
        }

        debug!("Completed range for job {}: {:?}", job_id, range);
    }

    /// Release all ranges for a failed GPU
    pub fn release_gpu_ranges(&self, gpu_index: usize) {
        let mut ranges_to_recycle = Vec::new();

        // Find all ranges for this GPU
        {
            let active = self.active_ranges.read();
            for (job_id, ranges) in active.iter() {
                for range in ranges {
                    if range.gpu_index == gpu_index {
                        ranges_to_recycle.push((job_id.clone(), range.clone()));
                    }
                }
            }
        }

        // Count ranges before recycling
        let count = ranges_to_recycle.len();

        // Recycle the ranges
        for (job_id, range) in ranges_to_recycle {
            self.complete_range(&job_id, range);
        }

        info!("Released {} ranges for GPU {}", count, gpu_index);
    }

    /// Clear all ranges for a job
    pub fn clear_job(&self, job_id: &str) {
        let mut offsets = self.job_offsets.write();
        let mut active = self.active_ranges.write();
        let mut recycled = self.recycled_ranges.write();

        offsets.remove(job_id);
        active.remove(job_id);
        recycled.remove(job_id);

        // Update stats
        let mut stats = self.stats.write();
        stats.active_jobs = offsets.len();
        stats.active_ranges = active.values().map(|v| v.len()).sum();

        info!("Cleared all ranges for job {}", job_id);
    }

    /// Get a recycled range if available
    fn get_recycled_range(
        &self,
        job_id: &str,
        gpu_index: usize,
        requested_size: u64,
    ) -> Option<NonceRange> {
        let mut recycled = self.recycled_ranges.write();
        let job_recycled = recycled.get_mut(job_id)?;

        // Find a suitable range
        for i in 0..job_recycled.len() {
            if let Some(range) = job_recycled.get(i).cloned() {
                if range.size() >= requested_size {
                    // Remove and return it
                    let mut range = job_recycled.remove(i)?;
                    range.gpu_index = gpu_index;

                    // If range is larger than needed, split it
                    if range.size() > requested_size * 2 {
                        let split_point = range.start + requested_size;
                        if let Some((left, right)) = range.split(split_point) {
                            // Keep the right part for later
                            job_recycled.push_back(right);
                            return Some(left);
                        }
                    }

                    return Some(range);
                }
            }
        }

        None
    }

    /// Check if a nonce has already been allocated
    pub fn is_nonce_allocated(&self, job_id: &str, nonce: u64) -> bool {
        let active = self.active_ranges.read();
        if let Some(ranges) = active.get(job_id) {
            for range in ranges {
                if range.contains(nonce) {
                    return true;
                }
            }
        }
        false
    }

    /// Get the next available nonce for a job
    pub fn get_next_nonce(&self, job_id: &str) -> u64 {
        let offsets = self.job_offsets.read();
        *offsets.get(job_id).unwrap_or(&self.config.initial_nonce)
    }

    /// Get statistics
    pub fn get_stats(&self) -> NonceStats {
        self.stats.read().clone()
    }

    /// Get active ranges for a job
    pub fn get_active_ranges(&self, job_id: &str) -> Vec<NonceRange> {
        let active = self.active_ranges.read();
        active.get(job_id).cloned().unwrap_or_default()
    }

    /// Calculate coverage percentage for a job
    pub fn get_coverage(&self, job_id: &str) -> f64 {
        let offsets = self.job_offsets.read();
        if let Some(offset) = offsets.get(job_id) {
            let coverage = *offset as f64 / self.config.max_nonce as f64;
            coverage * 100.0
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nonce_allocation() {
        let manager = NonceManager::new(NonceManagerConfig::default());

        // Allocate ranges for different GPUs
        let range1 = manager.allocate_range("job1", 0, Some(1000)).unwrap();
        let range2 = manager.allocate_range("job1", 1, Some(1000)).unwrap();

        // Ranges should not overlap
        assert_eq!(range1.start, 0);
        assert_eq!(range1.end, 1000);
        assert_eq!(range2.start, 1000);
        assert_eq!(range2.end, 2000);

        // Check allocation tracking
        assert!(manager.is_nonce_allocated("job1", 500));
        assert!(manager.is_nonce_allocated("job1", 1500));
        assert!(!manager.is_nonce_allocated("job1", 2500));
    }

    #[test]
    fn test_range_recycling() {
        let config = NonceManagerConfig {
            enable_recycling: true,
            ..Default::default()
        };
        let manager = NonceManager::new(config);

        // Allocate and complete a range
        let range1 = manager.allocate_range("job1", 0, Some(1000)).unwrap();
        manager.complete_range("job1", range1);

        // Next allocation should reuse the recycled range
        let range2 = manager.allocate_range("job1", 1, Some(500)).unwrap();
        assert_eq!(range2.start, 0); // Reused from recycled range
        assert_eq!(range2.gpu_index, 1); // But assigned to different GPU
    }

    #[test]
    fn test_gpu_failure_recovery() {
        let manager = NonceManager::new(NonceManagerConfig::default());

        // Allocate ranges for GPU 0
        let _range1 = manager.allocate_range("job1", 0, Some(1000)).unwrap();
        let _range2 = manager.allocate_range("job1", 0, Some(1000)).unwrap();

        // Simulate GPU failure
        manager.release_gpu_ranges(0);

        // Ranges should be available for recycling
        let stats = manager.get_stats();
        assert_eq!(stats.ranges_recycled, 2);
        assert_eq!(stats.nonces_recycled, 2000);
    }
}