/// Results export functionality (JSON, CSV, Markdown)
use std::fs::File;
use std::io::Write;
use std::path::Path;
use serde::{Deserialize, Serialize};
use csv;
use anyhow::Result;

use super::BenchmarkResults;

/// Export format options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportFormat {
    Json,
    Csv,
    Markdown,
}

/// JSON exporter
pub struct JsonExporter;

impl JsonExporter {
    /// Export results to JSON file
    pub fn export(results: &BenchmarkResults, path: impl AsRef<Path>) -> Result<()> {
        let json = serde_json::to_string_pretty(results)?;
        let mut file = File::create(path)?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }

    /// Export with custom formatting
    pub fn export_formatted(results: &BenchmarkResults, path: impl AsRef<Path>) -> Result<()> {
        let formatted = FormattedResults::from(results);
        let json = serde_json::to_string_pretty(&formatted)?;
        let mut file = File::create(path)?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }
}

/// CSV exporter
pub struct CsvExporter;

impl CsvExporter {
    /// Export results to CSV file
    pub fn export(results: &BenchmarkResults, path: impl AsRef<Path>) -> Result<()> {
        let mut wtr = csv::Writer::from_path(path.as_ref())?;

        // Write summary header
        wtr.write_record(&[
            "Metric",
            "Value",
            "Unit",
        ])?;

        // Write summary metrics
        wtr.write_record(&[
            "Session ID",
            &results.session_id,
            "",
        ])?;

        wtr.write_record(&[
            "Start Time",
            &results.start_time.to_string(),
            "",
        ])?;

        wtr.write_record(&[
            "End Time",
            &results.end_time.to_string(),
            "",
        ])?;

        wtr.write_record(&[
            "Duration",
            &format!("{}", (results.end_time - results.start_time).num_seconds()),
            "seconds",
        ])?;

        // Hashrate statistics
        wtr.write_record(&[
            "Current Hashrate",
            &format!("{:.2}", results.hashrate_stats.current / 1_000_000.0),
            "MH/s",
        ])?;

        wtr.write_record(&[
            "Average Hashrate",
            &format!("{:.2}", results.hashrate_stats.average / 1_000_000.0),
            "MH/s",
        ])?;

        wtr.write_record(&[
            "Peak Hashrate",
            &format!("{:.2}", results.hashrate_stats.max / 1_000_000.0),
            "MH/s",
        ])?;

        wtr.write_record(&[
            "Minimum Hashrate",
            &format!("{:.2}", results.hashrate_stats.min / 1_000_000.0),
            "MH/s",
        ])?;

        // Power metrics
        wtr.write_record(&[
            "Total Power",
            &format!("{:.1}", results.power_metrics.total_power),
            "W",
        ])?;

        wtr.write_record(&[
            "Efficiency",
            &format!("{:.2}", results.power_metrics.efficiency_hw),
            "H/W",
        ])?;

        // Share statistics
        wtr.write_record(&[
            "Shares Accepted",
            &format!("{}", results.share_statistics.accepted),
            "",
        ])?;

        wtr.write_record(&[
            "Shares Rejected",
            &format!("{}", results.share_statistics.rejected),
            "",
        ])?;

        wtr.write_record(&[
            "Acceptance Rate",
            &format!("{:.2}", results.share_statistics.acceptance_rate),
            "%",
        ])?;

        wtr.flush()?;

        // Export time series to separate file
        let ts_path = path.as_ref().with_extension("timeseries.csv");
        Self::export_time_series(results, ts_path)?;

        Ok(())
    }

    /// Export time-series data
    fn export_time_series(results: &BenchmarkResults, path: impl AsRef<Path>) -> Result<()> {
        let mut wtr = csv::Writer::from_path(path)?;

        // Write header
        wtr.write_record(&[
            "Timestamp",
            "Hashrate (MH/s)",
            "Power (W)",
            "Temperature (C)",
            "GPU Util (%)",
            "Memory Util (%)",
            "Shares Accepted",
            "Shares Rejected",
        ])?;

        // Write data points
        for snapshot in &results.time_series {
            wtr.write_record(&[
                &snapshot.timestamp.to_string(),
                &format!("{:.2}", snapshot.hashrate_mhs),
                &format!("{:.1}", snapshot.power_watts),
                &format!("{:.1}", snapshot.temperature_c),
                &format!("{:.1}", snapshot.gpu_utilization),
                &format!("{:.1}", snapshot.memory_utilization),
                &format!("{}", snapshot.shares_accepted),
                &format!("{}", snapshot.shares_rejected),
            ])?;
        }

        wtr.flush()?;
        Ok(())
    }
}

/// Markdown report generator
pub struct MarkdownExporter;

impl MarkdownExporter {
    /// Export results as Markdown report
    pub fn export(results: &BenchmarkResults, path: impl AsRef<Path>) -> Result<()> {
        let mut content = String::new();

        // Title
        content.push_str("# MineOS Benchmark Report\n\n");

        // Session info
        content.push_str("## Session Information\n\n");
        content.push_str(&format!("- **Session ID**: {}\n", results.session_id));
        content.push_str(&format!("- **Start Time**: {}\n", results.start_time));
        content.push_str(&format!("- **End Time**: {}\n", results.end_time));
        content.push_str(&format!("- **Duration**: {} seconds\n\n",
            (results.end_time - results.start_time).num_seconds()));

        // Hardware info
        content.push_str("## Hardware Configuration\n\n");
        content.push_str(&format!("- **GPU Count**: {}\n", results.hardware_info.gpu_count));
        content.push_str(&format!("- **GPU Models**: {}\n", results.hardware_info.gpu_models.join(", ")));
        content.push_str(&format!("- **Driver Version**: {}\n", results.hardware_info.driver_version));
        if let Some(cuda) = &results.hardware_info.cuda_version {
            content.push_str(&format!("- **CUDA Version**: {}\n", cuda));
        }
        content.push_str(&format!("- **CPU**: {}\n", results.hardware_info.cpu_model));
        content.push_str(&format!("- **System Memory**: {} GB\n\n",
            results.hardware_info.system_memory / (1024 * 1024 * 1024)));

        // Performance summary
        content.push_str("## Performance Summary\n\n");
        content.push_str("### Hashrate Statistics\n\n");
        content.push_str("| Metric | Value |\n");
        content.push_str("|--------|-------|\n");
        content.push_str(&format!("| Current | {:.2} MH/s |\n",
            results.hashrate_stats.current / 1_000_000.0));
        content.push_str(&format!("| Average | {:.2} MH/s |\n",
            results.hashrate_stats.average / 1_000_000.0));
        content.push_str(&format!("| Peak | {:.2} MH/s |\n",
            results.hashrate_stats.max / 1_000_000.0));
        content.push_str(&format!("| Minimum | {:.2} MH/s |\n",
            results.hashrate_stats.min / 1_000_000.0));
        content.push_str(&format!("| Std Deviation | {:.2} MH/s |\n\n",
            results.hashrate_stats.std_deviation / 1_000_000.0));

        // Power and efficiency
        content.push_str("### Power & Efficiency\n\n");
        content.push_str("| Metric | Value |\n");
        content.push_str("|--------|-------|\n");
        content.push_str(&format!("| Total Power | {:.1} W |\n",
            results.power_metrics.total_power));
        content.push_str(&format!("| Efficiency | {:.2} H/W |\n",
            results.power_metrics.efficiency_hw));
        content.push_str(&format!("| MH/J | {:.3} |\n\n",
            results.power_metrics.efficiency_mhj));

        // Share statistics
        content.push_str("### Share Statistics\n\n");
        content.push_str("| Metric | Value |\n");
        content.push_str("|--------|-------|\n");
        content.push_str(&format!("| Accepted | {} |\n", results.share_statistics.accepted));
        content.push_str(&format!("| Rejected | {} |\n", results.share_statistics.rejected));
        content.push_str(&format!("| Stale | {} |\n", results.share_statistics.stale));
        content.push_str(&format!("| Acceptance Rate | {:.2}% |\n\n",
            results.share_statistics.acceptance_rate));

        // Thermal data
        content.push_str("### Thermal Performance\n\n");
        content.push_str(&format!("- **Average Temperature**: {:.1}°C\n",
            results.power_metrics.avg_temperature));
        content.push_str(&format!("- **Throttle Events**: {}\n",
            results.thermal_data.throttle_events.len()));
        content.push_str(&format!("- **Performance Loss**: {:.1}%\n",
            results.thermal_data.impact_analysis.performance_loss_percent));
        content.push_str(&format!("- **Cooling Efficiency**: {:.1}%\n\n",
            results.thermal_data.cooling_efficiency));

        // T-Rex comparison if available
        if let Some(comparison) = &results.comparison_data {
            content.push_str("## T-Rex Comparison\n\n");
            content.push_str("| Miner | Hashrate | Power | Efficiency |\n");
            content.push_str("|-------|----------|-------|------------|\n");
            content.push_str(&format!("| MineOS | {:.2} MH/s | {:.1} W | {:.2} H/W |\n",
                comparison.mineos.hashrate / 1_000_000.0,
                comparison.mineos.power_watts,
                comparison.mineos.efficiency));
            content.push_str(&format!("| T-Rex | {:.2} MH/s | {:.1} W | {:.2} H/W |\n",
                comparison.trex.hashrate / 1_000_000.0,
                comparison.trex.power_watts,
                comparison.trex.efficiency));
            content.push_str(&format!("| **Difference** | **{:+.1}%** | **{:+.1} W** | **{:+.1}%** |\n\n",
                comparison.performance_delta,
                comparison.details.power_savings,
                comparison.efficiency_delta));
        }

        // Write to file
        let mut file = File::create(path)?;
        file.write_all(content.as_bytes())?;

        Ok(())
    }
}

/// Formatted results for pretty JSON output
#[derive(Serialize)]
struct FormattedResults {
    pub benchmark_info: BenchmarkInfo,
    pub performance: PerformanceSection,
    pub efficiency: EfficiencySection,
    pub thermal: ThermalSection,
    pub shares: ShareSection,
    pub comparison: Option<ComparisonSection>,
}

#[derive(Serialize)]
struct BenchmarkInfo {
    pub session_id: String,
    pub duration_seconds: i64,
    pub hardware: String,
}

#[derive(Serialize)]
struct PerformanceSection {
    pub hashrate_mhs: HashrateDisplay,
    pub stability_score: f64,
}

#[derive(Serialize)]
struct HashrateDisplay {
    pub current: f64,
    pub average: f64,
    pub peak: f64,
    pub minimum: f64,
}

#[derive(Serialize)]
struct EfficiencySection {
    pub power_watts: f32,
    pub hash_per_watt: f64,
    pub mh_per_joule: f64,
}

#[derive(Serialize)]
struct ThermalSection {
    pub avg_temperature_c: f32,
    pub throttle_events: usize,
    pub cooling_efficiency: f64,
}

#[derive(Serialize)]
struct ShareSection {
    pub accepted: u64,
    pub rejected: u64,
    pub acceptance_rate_percent: f64,
}

#[derive(Serialize)]
struct ComparisonSection {
    pub vs_trex: ComparisonDisplay,
}

#[derive(Serialize)]
struct ComparisonDisplay {
    pub hashrate_advantage_percent: f64,
    pub power_savings_watts: f32,
    pub efficiency_improvement_percent: f64,
}

impl From<&BenchmarkResults> for FormattedResults {
    fn from(results: &BenchmarkResults) -> Self {
        let hardware = format!("{} x {}",
            results.hardware_info.gpu_count,
            results.hardware_info.gpu_models.first().unwrap_or(&"Unknown GPU".to_string()));

        let comparison = results.comparison_data.as_ref().map(|comp| {
            ComparisonSection {
                vs_trex: ComparisonDisplay {
                    hashrate_advantage_percent: comp.performance_delta,
                    power_savings_watts: comp.details.power_savings,
                    efficiency_improvement_percent: comp.efficiency_delta,
                },
            }
        });

        FormattedResults {
            benchmark_info: BenchmarkInfo {
                session_id: results.session_id.clone(),
                duration_seconds: (results.end_time - results.start_time).num_seconds(),
                hardware,
            },
            performance: PerformanceSection {
                hashrate_mhs: HashrateDisplay {
                    current: results.hashrate_stats.current / 1_000_000.0,
                    average: results.hashrate_stats.average / 1_000_000.0,
                    peak: results.hashrate_stats.max / 1_000_000.0,
                    minimum: results.hashrate_stats.min / 1_000_000.0,
                },
                stability_score: 100.0 - (results.hashrate_stats.std_deviation / results.hashrate_stats.average * 100.0).min(100.0),
            },
            efficiency: EfficiencySection {
                power_watts: results.power_metrics.total_power,
                hash_per_watt: results.power_metrics.efficiency_hw,
                mh_per_joule: results.power_metrics.efficiency_mhj,
            },
            thermal: ThermalSection {
                avg_temperature_c: results.power_metrics.avg_temperature,
                throttle_events: results.thermal_data.throttle_events.len(),
                cooling_efficiency: results.thermal_data.cooling_efficiency,
            },
            shares: ShareSection {
                accepted: results.share_statistics.accepted,
                rejected: results.share_statistics.rejected,
                acceptance_rate_percent: results.share_statistics.acceptance_rate,
            },
            comparison,
        }
    }
}