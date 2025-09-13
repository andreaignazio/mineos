use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use mineos_hash::{Hash256, BlockHeader, MiningResult};
use mineos_stratum::protocol::Share;

/// Result of share validation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationResult {
    /// Share is valid and should be submitted
    Valid,

    /// Share is valid but stale (job expired)
    Stale,

    /// Share is a duplicate
    Duplicate,

    /// Share doesn't meet difficulty target
    BelowTarget,

    /// Share has invalid hash
    InvalidHash,

    /// Share nonce is out of assigned range
    InvalidNonce,

    /// Share is for unknown job
    UnknownJob,
}

/// Validated share ready for submission
#[derive(Debug, Clone)]
pub struct ValidatedShare {
    /// Original share data
    pub share: Share,

    /// Mining result with hash and nonce
    pub result: MiningResult,

    /// Job ID this share is for
    pub job_id: String,

    /// GPU that found this share
    pub gpu_index: usize,

    /// Timestamp when share was found
    pub found_at: Instant,

    /// Difficulty of this share
    pub difficulty: f64,
}

/// Configuration for share validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareValidatorConfig {
    /// Enable duplicate detection
    pub detect_duplicates: bool,

    /// Maximum age for duplicate detection cache (seconds)
    pub duplicate_cache_ttl: u64,

    /// Maximum number of shares in duplicate cache
    pub duplicate_cache_size: usize,

    /// Enable nonce range validation
    pub validate_nonce_range: bool,

    /// Maximum job age for stale detection (seconds)
    pub max_job_age: u64,

    /// Enable fast CPU verification
    pub fast_verify: bool,
}

impl Default for ShareValidatorConfig {
    fn default() -> Self {
        Self {
            detect_duplicates: true,
            duplicate_cache_ttl: 300, // 5 minutes
            duplicate_cache_size: 10000,
            validate_nonce_range: true,
            max_job_age: 120, // 2 minutes
            fast_verify: true,
        }
    }
}

/// Share history entry for duplicate detection
#[derive(Debug, Clone)]
struct ShareHistoryEntry {
    nonce: u64,
    hash: Hash256,
    job_id: String,
    timestamp: Instant,
}

/// Validates shares before pool submission
pub struct ShareValidator {
    /// Configuration
    config: ShareValidatorConfig,

    /// Share history for duplicate detection
    share_history: RwLock<VecDeque<ShareHistoryEntry>>,

    /// Set of recent share hashes for fast lookup
    recent_hashes: RwLock<HashSet<Hash256>>,

    /// Active job information
    active_jobs: RwLock<HashSet<String>>,

    /// Nonce range validator (optional)
    nonce_validator: Option<Arc<crate::nonce_manager::NonceManager>>,

    /// Statistics
    stats: RwLock<ValidationStats>,
}

/// Validation statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationStats {
    pub total_shares_validated: u64,
    pub valid_shares: u64,
    pub stale_shares: u64,
    pub duplicate_shares: u64,
    pub invalid_shares: u64,
    pub below_target_shares: u64,
    pub out_of_range_nonces: u64,
}

impl ShareValidator {
    /// Create a new share validator
    pub fn new(config: ShareValidatorConfig) -> Self {
        Self {
            config,
            share_history: RwLock::new(VecDeque::with_capacity(10000)),
            recent_hashes: RwLock::new(HashSet::new()),
            active_jobs: RwLock::new(HashSet::new()),
            nonce_validator: None,
            stats: RwLock::new(ValidationStats::default()),
        }
    }

    /// Set the nonce manager for range validation
    pub fn set_nonce_manager(&mut self, manager: Arc<crate::nonce_manager::NonceManager>) {
        self.nonce_validator = Some(manager);
    }

    /// Register a new job as active
    pub fn register_job(&self, job_id: String) {
        let mut jobs = self.active_jobs.write();
        jobs.insert(job_id);
    }

    /// Unregister a job (mark as inactive)
    pub fn unregister_job(&self, job_id: &str) {
        let mut jobs = self.active_jobs.write();
        jobs.remove(job_id);
    }

    /// Validate a mining result
    pub fn validate_result(
        &self,
        result: &MiningResult,
        header: &BlockHeader,
        target: &Hash256,
        job_id: &str,
        _gpu_index: usize,
    ) -> ValidationResult {
        let mut stats = self.stats.write();
        stats.total_shares_validated += 1;

        // Check if job is known
        if !self.is_job_active(job_id) {
            warn!("Share for unknown job: {}", job_id);
            stats.invalid_shares += 1;
            return ValidationResult::UnknownJob;
        }

        // Check nonce range if validator is set
        if self.config.validate_nonce_range {
            if let Some(ref nonce_mgr) = self.nonce_validator {
                if !nonce_mgr.is_nonce_allocated(job_id, result.nonce) {
                    warn!("Nonce {} out of assigned range for job {}", result.nonce, job_id);
                    stats.out_of_range_nonces += 1;
                    return ValidationResult::InvalidNonce;
                }
            }
        }

        // Check for duplicates
        if self.config.detect_duplicates && self.is_duplicate(result, job_id) {
            debug!("Duplicate share detected: nonce={}", result.nonce);
            stats.duplicate_shares += 1;
            return ValidationResult::Duplicate;
        }

        // Verify the hash meets target
        if !result.hash.meets_target(target) {
            debug!("Share below target: {} > {}", result.hash.to_hex(), target.to_hex());
            stats.below_target_shares += 1;
            return ValidationResult::BelowTarget;
        }

        // Fast CPU verification if enabled
        if self.config.fast_verify {
            if !self.verify_hash_fast(result, header) {
                warn!("Share failed fast verification");
                stats.invalid_shares += 1;
                return ValidationResult::InvalidHash;
            }
        }

        // Add to history for duplicate detection
        self.add_to_history(result, job_id);

        stats.valid_shares += 1;
        ValidationResult::Valid
    }

    /// Create a validated share for pool submission
    pub fn create_validated_share(
        &self,
        result: MiningResult,
        job_id: String,
        gpu_index: usize,
        worker_name: String,
        extra_nonce2: String,
        ntime: String,
    ) -> ValidatedShare {
        // Calculate share difficulty
        let difficulty = self.calculate_difficulty(&result.hash);

        // Create stratum share
        let share = Share {
            worker_name,
            job_id: job_id.clone(),
            extra_nonce2,
            ntime,
            nonce: format!("{:016x}", result.nonce),
            version_rolling_mask: None,
        };

        ValidatedShare {
            share,
            result,
            job_id,
            gpu_index,
            found_at: Instant::now(),
            difficulty,
        }
    }

    /// Check if a job is active
    fn is_job_active(&self, job_id: &str) -> bool {
        let jobs = self.active_jobs.read();
        jobs.contains(job_id)
    }

    /// Check if a share is a duplicate
    fn is_duplicate(&self, result: &MiningResult, job_id: &str) -> bool {
        // Quick check in hash set
        let hashes = self.recent_hashes.read();
        if hashes.contains(&result.hash) {
            return true;
        }

        // Detailed check in history
        let history = self.share_history.read();
        for entry in history.iter() {
            if entry.nonce == result.nonce && entry.job_id == job_id {
                return true;
            }
        }

        false
    }

    /// Add a share to history
    fn add_to_history(&self, result: &MiningResult, job_id: &str) {
        let entry = ShareHistoryEntry {
            nonce: result.nonce,
            hash: result.hash.clone(),
            job_id: job_id.to_string(),
            timestamp: Instant::now(),
        };

        let mut history = self.share_history.write();
        let mut hashes = self.recent_hashes.write();

        // Add to history
        history.push_back(entry.clone());
        hashes.insert(result.hash.clone());

        // Cleanup old entries
        let cutoff = Instant::now() - Duration::from_secs(self.config.duplicate_cache_ttl);
        while let Some(front) = history.front() {
            if front.timestamp < cutoff || history.len() > self.config.duplicate_cache_size {
                if let Some(old) = history.pop_front() {
                    hashes.remove(&old.hash);
                }
            } else {
                break;
            }
        }
    }

    /// Fast CPU verification (simplified check)
    fn verify_hash_fast(&self, result: &MiningResult, _header: &BlockHeader) -> bool {
        // In production, this would do actual hash verification
        // For now, just check basic validity

        // Check nonce is valid
        if result.nonce == 0 || result.nonce == u64::MAX {
            return false;
        }

        // Check hash is not all zeros or all ones
        let hash_bytes = result.hash.as_bytes();
        let all_zeros = hash_bytes.iter().all(|&b| b == 0);
        let all_ones = hash_bytes.iter().all(|&b| b == 0xFF);

        if all_zeros || all_ones {
            return false;
        }

        // In production, would verify:
        // 1. Hash matches the header + nonce
        // 2. Mix hash is correct (for algorithms that use it)
        // 3. Difficulty calculation is correct

        true
    }

    /// Calculate difficulty from hash
    fn calculate_difficulty(&self, hash: &Hash256) -> f64 {
        // Simplified difficulty calculation
        // In production, this would use the actual difficulty formula
        let hash_bytes = hash.as_bytes();

        // Count leading zeros
        let mut leading_zeros = 0;
        for byte in hash_bytes {
            if *byte == 0 {
                leading_zeros += 8;
            } else {
                leading_zeros += byte.leading_zeros();
                break;
            }
        }

        // Approximate difficulty based on leading zeros
        2f64.powi(leading_zeros as i32)
    }

    /// Get validation statistics
    pub fn get_stats(&self) -> ValidationStats {
        self.stats.read().clone()
    }

    /// Clear share history
    pub fn clear_history(&self) {
        let mut history = self.share_history.write();
        let mut hashes = self.recent_hashes.write();

        history.clear();
        hashes.clear();

        info!("Cleared share validation history");
    }

    /// Get duplicate detection cache size
    pub fn get_cache_size(&self) -> usize {
        self.share_history.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_result(nonce: u64) -> MiningResult {
        let mut hash_bytes = [0u8; 32];
        hash_bytes[0] = (nonce & 0xFF) as u8;

        MiningResult {
            nonce,
            hash: Hash256::from_bytes(hash_bytes),
            mix_hash: None,
        }
    }

    #[test]
    fn test_duplicate_detection() {
        let validator = ShareValidator::new(ShareValidatorConfig::default());
        let header = BlockHeader::default();
        let target = Hash256::from_bytes([0xFF; 32]); // Easy target

        validator.register_job("job1".to_string());

        let result = create_test_result(12345);

        // First submission should be valid
        let validation1 = validator.validate_result(&result, &header, &target, "job1", 0);
        assert_eq!(validation1, ValidationResult::Valid);

        // Second submission should be duplicate
        let validation2 = validator.validate_result(&result, &header, &target, "job1", 0);
        assert_eq!(validation2, ValidationResult::Duplicate);
    }

    #[test]
    fn test_target_validation() {
        let validator = ShareValidator::new(ShareValidatorConfig::default());
        let header = BlockHeader::default();
        let mut target_bytes = [0u8; 32];
        target_bytes[31] = 0x01; // Very difficult target
        let target = Hash256::from_bytes(target_bytes);

        validator.register_job("job1".to_string());

        // Create a result with hash that doesn't meet target
        // The hash will have 0x9F in first byte, but we need the last byte to be > 0x01
        let mut hash_bytes = [0u8; 32];
        hash_bytes[31] = 0x02; // This is greater than target[31] = 0x01
        let result = MiningResult {
            nonce: 99999,
            hash: Hash256::from_bytes(hash_bytes),
            mix_hash: None,
        };

        let validation = validator.validate_result(&result, &header, &target, "job1", 0);
        assert_eq!(validation, ValidationResult::BelowTarget);
    }

    #[test]
    fn test_unknown_job() {
        let validator = ShareValidator::new(ShareValidatorConfig::default());
        let header = BlockHeader::default();
        let target = Hash256::from_bytes([0xFF; 32]);

        // Don't register the job
        let result = create_test_result(55555);

        let validation = validator.validate_result(&result, &header, &target, "unknown_job", 0);
        assert_eq!(validation, ValidationResult::UnknownJob);
    }
}