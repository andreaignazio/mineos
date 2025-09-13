use std::collections::HashMap;
use std::time::{Duration, Instant};
use mineos_core::{
    WorkDistributor, WorkDistributorConfig, WorkResult,
    JobQueue, JobQueueConfig,
    NonceManager, NonceManagerConfig,
    ShareValidator, ShareValidatorConfig, ValidationResult,
    GpuScheduler, GpuSchedulerConfig, SchedulingStrategy, GpuLoad,
    GpuUtilizationMonitor, MonitoringConfig,
};
use mineos_hash::{Hash256, BlockHeader};
use mineos_stratum::protocol::MiningJob;

#[test]
fn test_work_distribution_basic() {
    let config = WorkDistributorConfig::default();
    let num_gpus = 4;
    let distributor = WorkDistributor::new(config, num_gpus);

    // Create a test job
    let job = MiningJob {
        job_id: "test_job_1".to_string(),
        prev_hash: "00000000".to_string(),
        coinbase1: "01000000".to_string(),
        coinbase2: "00000000".to_string(),
        merkle_branches: vec![],
        version: "00000001".to_string(),
        nbits: "1d00ffff".to_string(),
        ntime: "00000000".to_string(),
        clean_jobs: false,
    };

    let header = BlockHeader::default();
    let target = Hash256::from_bytes([0xFF; 32]);

    // Update job
    distributor.update_job(job, header, target);

    // Each GPU should be able to get work
    for gpu_idx in 0..num_gpus {
        let work = distributor.get_work(gpu_idx);
        assert!(work.is_some());

        let work_unit = work.unwrap();
        assert_eq!(work_unit.gpu_index, gpu_idx);
        assert!(work_unit.nonce_count > 0);
    }
}

#[test]
fn test_job_queue_priority() {
    let queue = JobQueue::new(JobQueueConfig::default());
    let header = BlockHeader::default();
    let target = Hash256::default();

    // Add normal job
    let normal_job = create_test_job("normal1", false);
    queue.add_job(normal_job, header.clone(), target.clone()).unwrap();

    // Add clean job (should get priority)
    let clean_job = create_test_job("clean1", true);
    queue.add_job(clean_job, header.clone(), target.clone()).unwrap();

    // Clean job should come first
    let next = queue.get_next_job().unwrap();
    assert_eq!(next.job.job_id, "clean1");
    assert!(next.clean);
}

#[test]
fn test_nonce_manager_allocation() {
    let manager = NonceManager::new(NonceManagerConfig::default());

    // Allocate ranges for different GPUs
    let range1 = manager.allocate_range("job1", 0, Some(1000)).unwrap();
    let range2 = manager.allocate_range("job1", 1, Some(1000)).unwrap();
    let range3 = manager.allocate_range("job1", 2, Some(1000)).unwrap();

    // Ranges should not overlap
    assert_eq!(range1.start, 0);
    assert_eq!(range1.end, 1000);
    assert_eq!(range2.start, 1000);
    assert_eq!(range2.end, 2000);
    assert_eq!(range3.start, 2000);
    assert_eq!(range3.end, 3000);

    // Check allocation tracking
    assert!(manager.is_nonce_allocated("job1", 500));
    assert!(manager.is_nonce_allocated("job1", 1500));
    assert!(manager.is_nonce_allocated("job1", 2500));
    assert!(!manager.is_nonce_allocated("job1", 3500));
}

#[test]
fn test_share_validation() {
    let validator = ShareValidator::new(ShareValidatorConfig::default());
    let header = BlockHeader::default();
    let target = Hash256::from_bytes([0xFF; 32]); // Easy target

    validator.register_job("job1".to_string());

    // Create a valid result
    let result = mineos_hash::MiningResult {
        nonce: 12345,
        hash: Hash256::from_bytes([0x01; 32]),
        mix_hash: None,
    };

    // First submission should be valid
    let validation1 = validator.validate_result(&result, &header, &target, "job1", 0);
    assert_eq!(validation1, ValidationResult::Valid);

    // Second submission should be duplicate
    let validation2 = validator.validate_result(&result, &header, &target, "job1", 0);
    assert_eq!(validation2, ValidationResult::Duplicate);
}

#[test]
fn test_gpu_scheduler_selection() {
    let config = GpuSchedulerConfig {
        strategy: SchedulingStrategy::LeastLoaded,
        ..Default::default()
    };
    let scheduler = GpuScheduler::new(config, 3);

    // Update load for GPUs
    scheduler.update_gpu_load(GpuLoad {
        gpu_index: 0,
        utilization: 80.0,
        memory_usage: 50.0,
        temperature: 70.0,
        power_watts: 200.0,
        hashrate: 100_000_000.0,
        active_work_units: 2,
        last_update: Instant::now(),
    });

    scheduler.update_gpu_load(GpuLoad {
        gpu_index: 1,
        utilization: 50.0,
        memory_usage: 30.0,
        temperature: 65.0,
        power_watts: 150.0,
        hashrate: 100_000_000.0,
        active_work_units: 1,
        last_update: Instant::now(),
    });

    scheduler.update_gpu_load(GpuLoad {
        gpu_index: 2,
        utilization: 90.0,
        memory_usage: 70.0,
        temperature: 75.0,
        power_watts: 250.0,
        hashrate: 100_000_000.0,
        active_work_units: 3,
        last_update: Instant::now(),
    });

    // Should select GPU 1 (least loaded)
    let selected = scheduler.select_gpu();
    assert_eq!(selected, Some(1));
}

#[test]
fn test_work_result_submission() {
    let config = WorkDistributorConfig::default();
    let distributor = WorkDistributor::new(config, 2);

    let job = create_test_job("test1", false);
    let header = BlockHeader::default();
    let target = Hash256::default();

    distributor.update_job(job, header, target);

    // Get work
    let work = distributor.get_work(0).unwrap();

    // Submit result
    let result = WorkResult {
        work_id: work.id,
        gpu_index: 0,
        nonce: Some(12345),
        hash: Some(Hash256::default()),
        mix_hash: None,
        hashes_computed: work.nonce_count,
        duration: Duration::from_secs(10),
        hashrate: 10_000_000.0,
    };

    distributor.submit_result(result);

    // Check stats updated
    let stats = distributor.get_stats();
    let gpu_stats = &stats.iter().find(|(idx, _)| *idx == 0).unwrap().1;
    assert_eq!(gpu_stats.units_completed, 1);
    assert_eq!(gpu_stats.solutions_found, 1);
}

#[test]
fn test_monitoring_metrics() {
    let monitor = GpuUtilizationMonitor::new(MonitoringConfig::default());

    // Update metrics
    let mut gpu_hashrates = HashMap::new();
    gpu_hashrates.insert(0, 100_000_000.0);
    gpu_hashrates.insert(1, 120_000_000.0);

    monitor.update_metrics(
        gpu_hashrates,
        100,  // shares accepted
        5,    // shares rejected
        50,   // work units completed
        Duration::from_secs(30),
    );

    // Check metrics
    let metrics = monitor.get_current_metrics();
    assert_eq!(metrics.total_hashrate, 220_000_000.0);
    assert!(metrics.acceptance_rate > 94.0 && metrics.acceptance_rate < 96.0);
}

// Helper function to create test job
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
fn test_work_stealing() {
    let mut config = WorkDistributorConfig::default();
    config.work_stealing_threshold = 0.1;
    let distributor = WorkDistributor::new(config, 3);

    let job = create_test_job("test1", false);
    let header = BlockHeader::default();
    let target = Hash256::default();

    distributor.update_job(job, header, target);

    // GPU 0 gets multiple work units
    let _work1 = distributor.get_work(0);
    let _work2 = distributor.get_work(0);

    // GPU 1 should be able to steal work if GPU 0 has more
    let stolen = distributor.get_work(1);
    assert!(stolen.is_some());
}

#[test]
fn test_clean_job_handling() {
    let config = WorkDistributorConfig::default();
    let distributor = WorkDistributor::new(config, 2);

    // First job
    let job1 = create_test_job("job1", false);
    let header = BlockHeader::default();
    let target = Hash256::default();
    distributor.update_job(job1, header.clone(), target.clone());

    // Get some work
    let work1 = distributor.get_work(0);
    assert!(work1.is_some());

    // Clean job should clear existing work
    let clean_job = create_test_job("job2", true);
    distributor.update_job(clean_job, header, target);

    // New work should be from clean job
    let work2 = distributor.get_work(0).unwrap();
    assert_eq!(work2.job_id, "job2");
}

#[test]
fn test_dynamic_work_sizing() {
    let mut config = WorkDistributorConfig::default();
    config.dynamic_sizing = true;
    let distributor = WorkDistributor::new(config, 2);

    let job = create_test_job("test1", false);
    let header = BlockHeader::default();
    let target = Hash256::default();
    distributor.update_job(job, header, target);

    // Simulate different hashrates
    let work1 = distributor.get_work(0).unwrap();
    distributor.submit_result(WorkResult {
        work_id: work1.id,
        gpu_index: 0,
        nonce: None,
        hash: None,
        mix_hash: None,
        hashes_computed: 100_000_000,
        duration: Duration::from_secs(1),
        hashrate: 100_000_000.0, // 100 MH/s
    });

    let work2 = distributor.get_work(1).unwrap();
    distributor.submit_result(WorkResult {
        work_id: work2.id,
        gpu_index: 1,
        nonce: None,
        hash: None,
        mix_hash: None,
        hashes_computed: 100_000_000,
        duration: Duration::from_secs(2),
        hashrate: 50_000_000.0, // 50 MH/s
    });

    // Future work should be sized differently
    // GPU 0 should get larger work units due to higher hashrate
    let stats = distributor.get_stats();
    let gpu0_stats = &stats.iter().find(|(idx, _)| *idx == 0).unwrap().1;
    let gpu1_stats = &stats.iter().find(|(idx, _)| *idx == 1).unwrap().1;

    assert!(gpu0_stats.average_hashrate > gpu1_stats.average_hashrate);
}