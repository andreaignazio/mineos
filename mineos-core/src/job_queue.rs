use std::collections::VecDeque;
use std::sync::Arc;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender, TryRecvError};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use mineos_stratum::protocol::MiningJob;
use mineos_hash::{Hash256, BlockHeader};

/// Priority level for jobs
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JobPriority {
    /// Clean job that should interrupt current work
    Critical = 3,

    /// High priority job
    High = 2,

    /// Normal priority job
    Normal = 1,

    /// Low priority job (backup work)
    Low = 0,
}

/// Queued mining job with metadata
#[derive(Debug, Clone)]
pub struct QueuedJob {
    /// The mining job from the pool
    pub job: Arc<MiningJob>,

    /// Block header for this job
    pub header: BlockHeader,

    /// Target difficulty
    pub target: Hash256,

    /// Job priority
    pub priority: JobPriority,

    /// Timestamp when job was received
    pub received_at: std::time::Instant,

    /// Whether this job should clear previous work
    pub clean: bool,
}

/// Configuration for the job queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobQueueConfig {
    /// Maximum number of jobs to keep in queue
    pub max_queue_size: usize,

    /// Enable priority-based scheduling
    pub enable_priority: bool,

    /// Maximum age for jobs in seconds
    pub max_job_age_secs: u64,

    /// Number of backup jobs to maintain
    pub backup_job_count: usize,
}

impl Default for JobQueueConfig {
    fn default() -> Self {
        Self {
            max_queue_size: 100,
            enable_priority: true,
            max_job_age_secs: 120, // 2 minutes
            backup_job_count: 3,
        }
    }
}

/// Thread-safe job queue for mining work
pub struct JobQueue {
    /// Configuration
    config: JobQueueConfig,

    /// High-priority job channel (for clean jobs)
    high_priority_tx: Sender<QueuedJob>,
    high_priority_rx: Receiver<QueuedJob>,

    /// Normal priority job channel
    normal_tx: Sender<QueuedJob>,
    normal_rx: Receiver<QueuedJob>,

    /// Current active job
    current_job: Arc<RwLock<Option<QueuedJob>>>,

    /// Backup jobs for when main queue is empty
    backup_jobs: Arc<RwLock<VecDeque<QueuedJob>>>,

    /// Statistics
    stats: Arc<RwLock<QueueStats>>,
}

/// Queue statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueStats {
    pub total_jobs_received: u64,
    pub total_jobs_processed: u64,
    pub clean_jobs_received: u64,
    pub jobs_dropped_age: u64,
    pub jobs_dropped_overflow: u64,
    pub current_queue_depth: usize,
}

impl JobQueue {
    /// Create a new job queue
    pub fn new(config: JobQueueConfig) -> Self {
        // Use bounded channels to prevent memory overflow
        let (high_tx, high_rx) = bounded(10);
        let (normal_tx, normal_rx) = bounded(config.max_queue_size);

        Self {
            config,
            high_priority_tx: high_tx,
            high_priority_rx: high_rx,
            normal_tx,
            normal_rx,
            current_job: Arc::new(RwLock::new(None)),
            backup_jobs: Arc::new(RwLock::new(VecDeque::with_capacity(10))),
            stats: Arc::new(RwLock::new(QueueStats::default())),
        }
    }

    /// Add a new job to the queue
    pub fn add_job(
        &self,
        job: MiningJob,
        header: BlockHeader,
        target: Hash256,
    ) -> Result<(), String> {
        // Update stats
        {
            let mut stats = self.stats.write();
            stats.total_jobs_received += 1;
            if job.clean_jobs {
                stats.clean_jobs_received += 1;
            }
        }

        // Determine priority
        let priority = if job.clean_jobs {
            JobPriority::Critical
        } else {
            JobPriority::Normal
        };

        // Create queued job
        let queued_job = QueuedJob {
            job: Arc::new(job.clone()),
            header,
            target,
            priority,
            received_at: std::time::Instant::now(),
            clean: job.clean_jobs,
        };

        // Route to appropriate channel
        let result = if priority == JobPriority::Critical {
            // Clean jobs go to high priority channel
            info!("Adding clean job {} to high priority queue", job.job_id);

            // Clear normal queue for clean jobs
            if job.clean_jobs {
                self.clear_normal_queue();
            }

            self.high_priority_tx.try_send(queued_job.clone())
        } else {
            debug!("Adding job {} to normal queue", job.job_id);
            self.normal_tx.try_send(queued_job.clone())
        };

        match result {
            Ok(_) => {
                // Update current queue depth
                let mut stats = self.stats.write();
                stats.current_queue_depth = self.get_queue_depth();

                // Also add to backup jobs if appropriate
                if !job.clean_jobs {
                    self.add_backup_job(queued_job);
                }

                Ok(())
            }
            Err(e) => {
                warn!("Failed to queue job: {}", e);
                let mut stats = self.stats.write();
                stats.jobs_dropped_overflow += 1;
                Err(format!("Queue full: {}", e))
            }
        }
    }

    /// Get the next job from the queue
    pub fn get_next_job(&self) -> Option<QueuedJob> {
        // Check high priority first
        match self.high_priority_rx.try_recv() {
            Ok(job) => {
                info!("Retrieved high priority job {}", job.job.job_id);
                self.set_current_job(job.clone());
                return Some(job);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                warn!("High priority channel disconnected");
            }
        }

        // Check normal priority
        match self.normal_rx.try_recv() {
            Ok(job) => {
                // Check if job is too old
                let age = job.received_at.elapsed();
                if age.as_secs() > self.config.max_job_age_secs {
                    debug!("Dropping aged job {} ({}s old)", job.job.job_id, age.as_secs());
                    let mut stats = self.stats.write();
                    stats.jobs_dropped_age += 1;
                    // Recursively try next job
                    return self.get_next_job();
                }

                debug!("Retrieved normal job {}", job.job.job_id);
                self.set_current_job(job.clone());
                return Some(job);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                warn!("Normal priority channel disconnected");
            }
        }

        // If no new jobs, return a backup job
        self.get_backup_job()
    }

    /// Get the current active job
    pub fn get_current_job(&self) -> Option<QueuedJob> {
        self.current_job.read().clone()
    }

    /// Set the current active job
    fn set_current_job(&self, job: QueuedJob) {
        *self.current_job.write() = Some(job);
        let mut stats = self.stats.write();
        stats.total_jobs_processed += 1;
        stats.current_queue_depth = self.get_queue_depth();
    }

    /// Clear the normal priority queue
    fn clear_normal_queue(&self) {
        let mut count = 0;
        while self.normal_rx.try_recv().is_ok() {
            count += 1;
        }
        if count > 0 {
            info!("Cleared {} jobs from normal queue for clean job", count);
        }
    }

    /// Add a job to the backup queue
    fn add_backup_job(&self, job: QueuedJob) {
        let mut backups = self.backup_jobs.write();

        // Remove old backups if at capacity
        while backups.len() >= self.config.backup_job_count {
            backups.pop_front();
        }

        backups.push_back(job);
    }

    /// Get a backup job when main queue is empty
    fn get_backup_job(&self) -> Option<QueuedJob> {
        let backups = self.backup_jobs.read();

        // Find the newest valid backup job
        for job in backups.iter().rev() {
            let age = job.received_at.elapsed();
            if age.as_secs() <= self.config.max_job_age_secs * 2 {
                debug!("Using backup job {}", job.job.job_id);
                return Some(job.clone());
            }
        }

        None
    }

    /// Get current queue depth
    fn get_queue_depth(&self) -> usize {
        let high_len = self.high_priority_rx.len();
        let normal_len = self.normal_rx.len();
        high_len + normal_len
    }

    /// Get queue statistics
    pub fn get_stats(&self) -> QueueStats {
        let stats = self.stats.read().clone();
        stats
    }

    /// Check if queue has work available
    pub fn has_work(&self) -> bool {
        !self.high_priority_rx.is_empty() ||
        !self.normal_rx.is_empty() ||
        !self.backup_jobs.read().is_empty()
    }

    /// Create a multi-producer multi-consumer channel pair
    pub fn create_channel_pair() -> (Sender<QueuedJob>, Receiver<QueuedJob>) {
        unbounded()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_job(job_id: &str, clean: bool) -> MiningJob {
        MiningJob {
            job_id: job_id.to_string(),
            prev_hash: "00000000".to_string(),
            coinbase1: "01000000".to_string(),
            coinbase2: "00000000".to_string(),
            merkle_branches: vec![],
            version: "00000001".to_string(),
            nbits: "1d00ffff".to_string(),
            ntime: "00000000".to_string(),
            clean_jobs: clean,
        }
    }

    #[test]
    fn test_job_queue_priority() {
        let queue = JobQueue::new(JobQueueConfig::default());
        let header = BlockHeader::default();
        let target = Hash256::default();

        // Add normal job
        queue.add_job(create_test_job("normal1", false), header.clone(), target.clone()).unwrap();

        // Add clean job (should get priority and clear normal queue)
        queue.add_job(create_test_job("clean1", true), header.clone(), target.clone()).unwrap();

        // Add another normal job after clean job
        queue.add_job(create_test_job("normal2", false), header.clone(), target.clone()).unwrap();

        // Clean job should come first
        let job1 = queue.get_next_job().unwrap();
        assert_eq!(job1.job.job_id, "clean1");

        // normal1 was cleared, so normal2 should be next
        let job2 = queue.get_next_job().unwrap();
        assert_eq!(job2.job.job_id, "normal2");
    }

    #[test]
    fn test_backup_jobs() {
        let mut config = JobQueueConfig::default();
        config.backup_job_count = 2;
        let queue = JobQueue::new(config);
        let header = BlockHeader::default();
        let target = Hash256::default();

        // Add jobs that will become backups
        queue.add_job(create_test_job("backup1", false), header.clone(), target.clone()).unwrap();
        queue.add_job(create_test_job("backup2", false), header.clone(), target.clone()).unwrap();
        queue.add_job(create_test_job("backup3", false), header.clone(), target.clone()).unwrap();

        // Consume all jobs from main queue
        queue.get_next_job();
        queue.get_next_job();
        queue.get_next_job();

        // Should still get backup jobs
        assert!(queue.get_backup_job().is_some());
    }
}