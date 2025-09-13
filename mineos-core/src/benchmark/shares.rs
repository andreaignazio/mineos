/// Share acceptance tracking and analysis
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

/// Share submission statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareStats {
    /// Total shares accepted
    pub accepted: u64,

    /// Total shares rejected
    pub rejected: u64,

    /// Total stale shares
    pub stale: u64,

    /// Share acceptance rate (%)
    pub acceptance_rate: f64,

    /// Average response time (ms)
    pub avg_response_time: f64,

    /// Rejection reasons breakdown
    pub rejection_reasons: RejectionBreakdown,

    /// Shares per minute
    pub shares_per_minute: f64,

    /// Effective hashrate from shares
    pub effective_hashrate: f64,
}

/// Breakdown of rejection reasons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectionBreakdown {
    pub invalid_nonce: u64,
    pub below_target: u64,
    pub duplicate: u64,
    pub job_not_found: u64,
    pub other: u64,
}

/// Share submission result
#[derive(Debug, Clone)]
pub struct ShareSubmission {
    pub timestamp: Instant,
    pub accepted: bool,
    pub response_time_ms: u64,
    pub rejection_reason: Option<RejectionReason>,
    pub difficulty: f64,
}

/// Rejection reason
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RejectionReason {
    InvalidNonce,
    BelowTarget,
    Duplicate,
    JobNotFound,
    Stale,
    Other(String),
}

/// Share acceptance tracker
pub struct ShareAcceptanceTracker {
    /// Atomic counters for thread-safe updates
    accepted_count: AtomicU64,
    rejected_count: AtomicU64,
    stale_count: AtomicU64,

    /// Submission history
    history: parking_lot::RwLock<VecDeque<ShareSubmission>>,

    /// Rejection reasons counter
    rejection_reasons: parking_lot::RwLock<RejectionBreakdown>,

    /// Start time for rate calculations
    start_time: Instant,

    /// Current difficulty
    current_difficulty: parking_lot::RwLock<f64>,
}

impl ShareAcceptanceTracker {
    /// Create a new tracker
    pub fn new() -> Self {
        Self {
            accepted_count: AtomicU64::new(0),
            rejected_count: AtomicU64::new(0),
            stale_count: AtomicU64::new(0),
            history: parking_lot::RwLock::new(VecDeque::with_capacity(10000)),
            rejection_reasons: parking_lot::RwLock::new(RejectionBreakdown {
                invalid_nonce: 0,
                below_target: 0,
                duplicate: 0,
                job_not_found: 0,
                other: 0,
            }),
            start_time: Instant::now(),
            current_difficulty: parking_lot::RwLock::new(1.0),
        }
    }

    /// Record an accepted share
    pub fn record_accepted(&self) {
        self.accepted_count.fetch_add(1, Ordering::Relaxed);
        self.record_submission(true, None, 0);
    }

    /// Record a rejected share
    pub fn record_rejected(&self) {
        self.rejected_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a stale share
    pub fn record_stale(&self) {
        self.stale_count.fetch_add(1, Ordering::Relaxed);
        self.record_rejection(RejectionReason::Stale, 0);
    }

    /// Record a share submission with details
    pub fn record_submission(
        &self,
        accepted: bool,
        rejection_reason: Option<RejectionReason>,
        response_time_ms: u64,
    ) {
        let submission = ShareSubmission {
            timestamp: Instant::now(),
            accepted,
            response_time_ms,
            rejection_reason: rejection_reason.clone(),
            difficulty: *self.current_difficulty.read(),
        };

        // Update history
        let mut history = self.history.write();
        history.push_back(submission);

        // Limit history size
        if history.len() > 10000 {
            history.pop_front();
        }

        // Update rejection reasons if applicable
        if let Some(reason) = rejection_reason {
            self.update_rejection_reason(reason);
        }
    }

    /// Record a rejection with reason
    pub fn record_rejection(&self, reason: RejectionReason, response_time_ms: u64) {
        self.rejected_count.fetch_add(1, Ordering::Relaxed);
        self.record_submission(false, Some(reason), response_time_ms);
    }

    /// Update rejection reason counter
    fn update_rejection_reason(&self, reason: RejectionReason) {
        let mut reasons = self.rejection_reasons.write();
        match reason {
            RejectionReason::InvalidNonce => reasons.invalid_nonce += 1,
            RejectionReason::BelowTarget => reasons.below_target += 1,
            RejectionReason::Duplicate => reasons.duplicate += 1,
            RejectionReason::JobNotFound => reasons.job_not_found += 1,
            RejectionReason::Stale => {
                // Already counted in stale_count
            }
            RejectionReason::Other(_) => reasons.other += 1,
        }
    }

    /// Set current difficulty
    pub fn set_difficulty(&self, difficulty: f64) {
        *self.current_difficulty.write() = difficulty;
    }

    /// Get accepted count
    pub fn get_accepted_count(&self) -> u64 {
        self.accepted_count.load(Ordering::Relaxed)
    }

    /// Get rejected count
    pub fn get_rejected_count(&self) -> u64 {
        self.rejected_count.load(Ordering::Relaxed)
    }

    /// Get comprehensive statistics
    pub fn get_statistics(&self) -> ShareStats {
        let accepted = self.accepted_count.load(Ordering::Relaxed);
        let rejected = self.rejected_count.load(Ordering::Relaxed);
        let stale = self.stale_count.load(Ordering::Relaxed);
        let total = accepted + rejected + stale;

        let acceptance_rate = if total > 0 {
            (accepted as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        // Calculate average response time
        let history = self.history.read();
        let avg_response_time = if !history.is_empty() {
            let sum: u64 = history.iter().map(|s| s.response_time_ms).sum();
            sum as f64 / history.len() as f64
        } else {
            0.0
        };

        // Calculate shares per minute
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let shares_per_minute = if elapsed > 0.0 {
            (total as f64 / elapsed) * 60.0
        } else {
            0.0
        };

        // Calculate effective hashrate from shares
        let effective_hashrate = self.calculate_effective_hashrate();

        ShareStats {
            accepted,
            rejected,
            stale,
            acceptance_rate,
            avg_response_time,
            rejection_reasons: self.rejection_reasons.read().clone(),
            shares_per_minute,
            effective_hashrate,
        }
    }

    /// Calculate effective hashrate from accepted shares
    fn calculate_effective_hashrate(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed < 1.0 {
            return 0.0;
        }

        let accepted = self.accepted_count.load(Ordering::Relaxed);
        let difficulty = *self.current_difficulty.read();

        // Effective hashrate = (shares * difficulty * 2^32) / time
        (accepted as f64 * difficulty * 4_294_967_296.0) / elapsed
    }

    /// Get recent share history
    pub fn get_recent_history(&self, duration: Duration) -> Vec<ShareSubmission> {
        let cutoff = Instant::now() - duration;
        let history = self.history.read();

        history
            .iter()
            .filter(|s| s.timestamp > cutoff)
            .cloned()
            .collect()
    }

    /// Analyze share patterns
    pub fn analyze_patterns(&self) -> SharePatternAnalysis {
        let history = self.history.read();

        if history.is_empty() {
            return SharePatternAnalysis::default();
        }

        // Group shares by time windows (1 minute)
        let mut windows: Vec<WindowStats> = Vec::new();
        let window_duration = Duration::from_secs(60);
        let mut current_window_start = history[0].timestamp;
        let mut current_window = WindowStats::default();

        for submission in history.iter() {
            if submission.timestamp > current_window_start + window_duration {
                windows.push(current_window);
                current_window = WindowStats::default();
                current_window_start = submission.timestamp;
            }

            if submission.accepted {
                current_window.accepted += 1;
            } else {
                current_window.rejected += 1;
            }
            current_window.total += 1;
        }

        if current_window.total > 0 {
            windows.push(current_window);
        }

        // Analyze patterns
        let acceptance_variance = self.calculate_variance(&windows);
        let trend = self.calculate_trend(&windows);
        let consistency_score = self.calculate_consistency(&windows);

        SharePatternAnalysis {
            acceptance_variance,
            trend,
            consistency_score,
            windows,
        }
    }

    /// Calculate variance in acceptance rates
    fn calculate_variance(&self, windows: &[WindowStats]) -> f64 {
        if windows.len() < 2 {
            return 0.0;
        }

        let rates: Vec<f64> = windows
            .iter()
            .map(|w| if w.total > 0 {
                w.accepted as f64 / w.total as f64
            } else {
                0.0
            })
            .collect();

        let mean = rates.iter().sum::<f64>() / rates.len() as f64;
        let variance = rates
            .iter()
            .map(|r| (r - mean).powi(2))
            .sum::<f64>() / rates.len() as f64;

        variance.sqrt() * 100.0 // Return as percentage
    }

    /// Calculate acceptance rate trend
    fn calculate_trend(&self, windows: &[WindowStats]) -> AcceptanceTrend {
        if windows.len() < 3 {
            return AcceptanceTrend::Stable;
        }

        let first_half: Vec<&WindowStats> = windows[..windows.len() / 2].iter().collect();
        let second_half: Vec<&WindowStats> = windows[windows.len() / 2..].iter().collect();

        let first_rate = Self::average_acceptance_rate(&first_half);
        let second_rate = Self::average_acceptance_rate(&second_half);

        if second_rate > first_rate * 1.05 {
            AcceptanceTrend::Improving
        } else if second_rate < first_rate * 0.95 {
            AcceptanceTrend::Declining
        } else {
            AcceptanceTrend::Stable
        }
    }

    /// Calculate average acceptance rate for windows
    fn average_acceptance_rate(windows: &[&WindowStats]) -> f64 {
        let total_accepted: u64 = windows.iter().map(|w| w.accepted).sum();
        let total_shares: u64 = windows.iter().map(|w| w.total).sum();

        if total_shares > 0 {
            total_accepted as f64 / total_shares as f64
        } else {
            0.0
        }
    }

    /// Calculate consistency score
    fn calculate_consistency(&self, windows: &[WindowStats]) -> f64 {
        if windows.is_empty() {
            return 0.0;
        }

        let target_rate = 0.98; // Target 98% acceptance
        let deviations: Vec<f64> = windows
            .iter()
            .map(|w| {
                let rate = if w.total > 0 {
                    w.accepted as f64 / w.total as f64
                } else {
                    0.0
                };
                (rate - target_rate).abs()
            })
            .collect();

        let avg_deviation = deviations.iter().sum::<f64>() / deviations.len() as f64;
        let consistency = (1.0 - avg_deviation.min(1.0)) * 100.0;

        consistency
    }

    /// Reset all statistics
    pub fn reset(&self) {
        self.accepted_count.store(0, Ordering::Relaxed);
        self.rejected_count.store(0, Ordering::Relaxed);
        self.stale_count.store(0, Ordering::Relaxed);
        self.history.write().clear();
        *self.rejection_reasons.write() = RejectionBreakdown {
            invalid_nonce: 0,
            below_target: 0,
            duplicate: 0,
            job_not_found: 0,
            other: 0,
        };
    }
}

/// Window statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct WindowStats {
    pub accepted: u64,
    pub rejected: u64,
    pub total: u64,
}

/// Share pattern analysis
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SharePatternAnalysis {
    /// Variance in acceptance rates
    pub acceptance_variance: f64,

    /// Trend in acceptance rates
    pub trend: AcceptanceTrend,

    /// Consistency score (0-100)
    pub consistency_score: f64,

    /// Per-window statistics
    pub windows: Vec<WindowStats>,
}

/// Acceptance rate trend
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum AcceptanceTrend {
    Improving,
    #[default]
    Stable,
    Declining,
}