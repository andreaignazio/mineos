pub mod miner;
pub mod work_distributor;
pub mod job_queue;
pub mod nonce_manager;
pub mod share_validator;
pub mod gpu_scheduler;
pub mod monitoring;
pub mod benchmark;

// Re-export main types
pub use miner::{MinerOrchestrator, MinerConfig, MinerStatus, MinerStats};
pub use work_distributor::{WorkDistributor, WorkDistributorConfig, WorkUnit, WorkResult, GpuStats};
pub use job_queue::{JobQueue, JobQueueConfig, JobPriority, QueuedJob};
pub use nonce_manager::{NonceManager, NonceManagerConfig, NonceRange};
pub use share_validator::{ShareValidator, ShareValidatorConfig, ValidationResult, ValidatedShare};
pub use gpu_scheduler::{GpuScheduler, GpuSchedulerConfig, SchedulingStrategy, GpuLoad};
pub use monitoring::{GpuUtilizationMonitor, MonitoringConfig, PerformanceMetrics};
pub use benchmark::{BenchmarkRunner, BenchmarkConfig, BenchmarkResults, BenchmarkSuite};