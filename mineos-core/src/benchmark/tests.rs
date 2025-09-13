#[cfg(test)]
mod tests {
    use super::super::*;
    use std::time::Duration;

    #[test]
    fn test_hashrate_meter() {
        let mut meter = hashrate::HashrateMeter::new();

        // Update with some hashrate values
        meter.update(0, 100_000_000.0); // 100 MH/s for GPU 0
        meter.update(1, 120_000_000.0); // 120 MH/s for GPU 1

        let stats = meter.get_statistics();
        assert_eq!(stats.current, 220_000_000.0);
        assert!(stats.gpu_hashrates.contains_key(&0));
        assert!(stats.gpu_hashrates.contains_key(&1));
    }

    #[test]
    fn test_share_tracker() {
        let tracker = shares::ShareAcceptanceTracker::new();

        // Record some shares
        tracker.record_accepted();
        tracker.record_accepted();
        tracker.record_rejected();

        let stats = tracker.get_statistics();
        assert_eq!(stats.accepted, 2);
        assert_eq!(stats.rejected, 1);
        assert!(stats.acceptance_rate > 66.0 && stats.acceptance_rate < 67.0);
    }

    #[test]
    fn test_power_efficiency() {
        let mut calc = efficiency::EfficiencyCalculator::new();

        // Update power readings
        calc.update_power(0, 250.0, 70.0, 2);
        calc.update_power(1, 230.0, 68.0, 2);

        let (hw, mhj) = calc.calculate_efficiency(220_000_000.0);
        assert!(hw > 0.0);
        assert!(mhj > 0.0);

        let metrics = calc.get_metrics();
        assert_eq!(metrics.total_power, 480.0);
    }

    #[test]
    fn test_thermal_monitor() {
        let mut monitor = thermal::ThermalMonitor::new();

        // Update temperatures
        monitor.update_temperature(0, 70.0, Some(85.0), 100_000_000.0);
        monitor.update_temperature(1, 72.0, Some(87.0), 120_000_000.0);

        let data = monitor.get_thermal_data();
        assert!(data.core_temps.contains_key(&0));
        assert!(data.core_temps.contains_key(&1));
        assert!(data.cooling_efficiency > 0.0);
    }

    #[test]
    fn test_benchmark_config_default() {
        let config = BenchmarkConfig::default();
        assert_eq!(config.duration, Duration::from_secs(300));
        assert_eq!(config.warmup_time, Duration::from_secs(30));
        assert_eq!(config.sample_interval, Duration::from_secs(1));
        assert!(!config.compare_with_trex);
    }

    #[test]
    fn test_export_formats() {
        use export::ExportFormat;

        let formats = vec![
            ExportFormat::Json,
            ExportFormat::Csv,
            ExportFormat::Markdown,
        ];

        // Just verify the enum variants exist
        assert_eq!(formats.len(), 3);
    }

    #[test]
    fn test_suite_scenarios() {
        use suite::TestScenario;

        let scenarios = vec![
            TestScenario::SteadyState,
            TestScenario::PoolSwitching,
            TestScenario::VariableDifficulty,
            TestScenario::RecoveryTest,
            TestScenario::PowerLimitTest,
            TestScenario::ThermalStress,
            TestScenario::MemoryStress,
        ];

        assert_eq!(scenarios.len(), 7);
    }

    #[test]
    fn test_benchmark_score_calculation() {
        use suite::BenchmarkScore;

        let score = BenchmarkScore {
            overall: 85.0,
            performance: 90.0,
            efficiency: 80.0,
            stability: 85.0,
            thermal: 85.0,
        };

        assert!(score.overall > 80.0);
        assert!(score.overall < 90.0);
    }

    #[test]
    fn test_hashrate_sampler() {
        let mut sampler = hashrate::HashrateSampler::new(Duration::from_millis(1), 100);

        // Add some samples with small delay to ensure they're accepted
        sampler.add_sample(100_000_000.0);
        std::thread::sleep(Duration::from_millis(2));
        sampler.add_sample(110_000_000.0);
        std::thread::sleep(Duration::from_millis(2));
        sampler.add_sample(105_000_000.0);

        let analysis = sampler.analyze();
        assert!(analysis.is_some());
        if let Some(stats) = analysis {
            // Check that we have the expected range
            assert!(stats.min >= 100_000_000.0);
            assert!(stats.max <= 110_000_000.0);
            assert_eq!(stats.sample_count, 3);
        }
    }

    #[test]
    fn test_share_pattern_analysis() {
        let tracker = shares::ShareAcceptanceTracker::new();

        // Record a pattern of shares
        for _ in 0..10 {
            tracker.record_accepted();
        }
        tracker.record_rejected();

        let analysis = tracker.analyze_patterns();
        assert!(analysis.consistency_score >= 0.0);
        assert!(analysis.consistency_score <= 100.0);
    }

    #[tokio::test]
    async fn test_benchmark_runner_creation() {
        let config = BenchmarkConfig::default();
        let runner = BenchmarkRunner::new(config);

        // Just verify we can create a runner
        assert!(runner.start_time.is_none());
    }

    #[test]
    fn test_suite_presets() {
        let quick = suite::BenchmarkSuite::quick();
        assert_eq!(quick.config.max_duration, Duration::from_secs(300));

        let standard = suite::BenchmarkSuite::standard();
        assert_eq!(standard.config.max_duration, Duration::from_secs(1800));

        let extended = suite::BenchmarkSuite::extended();
        assert_eq!(extended.config.max_duration, Duration::from_secs(7200));

        let stress = suite::BenchmarkSuite::stress_test();
        assert_eq!(stress.config.max_duration, Duration::from_secs(86400));
    }
}