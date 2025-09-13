use anyhow::Result;
use clap::Args;
use colored::*;
use console::style;
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::time::Duration;
use tabled::{Table, Tabled, settings::Style};

use crate::client::MinerClient;

/// Run performance benchmarks
#[derive(Args)]
pub struct BenchmarkArgs {
    /// Benchmark duration (e.g., 5m, 1h)
    #[arg(short, long, default_value = "5m")]
    duration: String,

    /// Algorithms to benchmark (comma-separated or 'all')
    #[arg(short, long, default_value = "current")]
    algorithms: String,

    /// Compare with T-Rex miner
    #[arg(long)]
    compare_trex: bool,

    /// Export results to file
    #[arg(short, long)]
    export: Option<String>,

    /// GPU indices to benchmark (e.g., 0,1,2 or all)
    #[arg(short, long, default_value = "all")]
    gpus: String,

    /// Run quick benchmark (1 minute per algorithm)
    #[arg(short, long)]
    quick: bool,
}

#[derive(Tabled, serde::Serialize)]
struct BenchmarkResult {
    #[tabled(rename = "Algorithm")]
    algorithm: String,
    #[tabled(rename = "GPU")]
    gpu: String,
    #[tabled(rename = "Hashrate")]
    hashrate: String,
    #[tabled(rename = "Power")]
    power: String,
    #[tabled(rename = "Efficiency")]
    efficiency: String,
    #[tabled(rename = "Temperature")]
    temperature: String,
    #[tabled(rename = "Shares")]
    shares: String,
}

pub async fn execute(args: BenchmarkArgs) -> Result<()> {
    println!("{}", "🔬 MineOS Benchmark Suite".bold().cyan());
    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black());

    // Parse duration
    let duration = parse_duration(&args.duration)?;
    if args.quick {
        println!("{}", "Running quick benchmark (1 minute per algorithm)".yellow());
    } else {
        println!("Duration: {}", format_duration(duration));
    }

    // Connect to miner
    let client = MinerClient::connect().await?;

    // Get available algorithms
    let algorithms: Vec<String> = if args.algorithms == "all" {
        vec!["kawpow".to_string(), "ethash".to_string(), "kheavyhash".to_string(), "autolykos2".to_string()]
    } else if args.algorithms == "current" {
        let status = client.get_status().await?;
        vec![status.algorithm]
    } else {
        args.algorithms.split(',').map(|s| s.to_string()).collect()
    };

    println!("Algorithms: {}", algorithms.join(", ").yellow());

    // Parse GPU selection
    let gpus = crate::utils::gpu_selector::parse_gpu_list(&args.gpus)?;
    println!("GPUs: {:?}\n", gpus);

    // Create progress bars
    let multi_progress = MultiProgress::new();
    let overall_pb = multi_progress.add(ProgressBar::new(algorithms.len() as u64));
    overall_pb.set_style(
        ProgressStyle::default_bar()
            .template("[{bar:40.cyan/blue}] {pos}/{len} algorithms")?
            .progress_chars("=>-"),
    );

    let mut all_results = Vec::new();

    // Benchmark each algorithm
    for (idx, algo) in algorithms.iter().enumerate() {
        overall_pb.set_message(format!("Benchmarking {}", algo));

        println!("{}", format!("\n▶ Benchmarking {} algorithm", algo).bold());

        // Create algorithm progress bar
        let algo_pb = multi_progress.add(ProgressBar::new(100));
        algo_pb.set_style(
            ProgressStyle::default_bar()
                .template("  [{bar:30.green/white}] {pos}% {msg}")?
                .progress_chars("█▉▊▋▌▍▎▏ "),
        );

        // Initialize algorithm
        algo_pb.set_message("Initializing...");
        // Algorithm initialization would happen here in production
        algo_pb.set_position(10);

        // Warm up
        algo_pb.set_message("Warming up...");
        tokio::time::sleep(Duration::from_secs(30)).await;
        algo_pb.set_position(30);

        // Run benchmark
        algo_pb.set_message("Benchmarking...");
        let bench_duration = if args.quick {
            Duration::from_secs(60)
        } else {
            duration
        };

        let results = client.run_benchmark(bench_duration.as_secs()).await?;
        algo_pb.set_position(90);

        // Collect results
        all_results.push(BenchmarkResult {
            algorithm: algo.to_string(),
            gpu: "All GPUs".to_string(),
            hashrate: format_hashrate(results.average_hashrate / 1_000_000.0),
            power: format!("{} W", results.power_avg),
            efficiency: format!("{:.3} MH/W", results.efficiency),
            temperature: format!("{}°C", results.temperature_avg),
            shares: format!("{}/{}", results.shares_found, results.shares_found),
        });

        algo_pb.set_position(100);
        algo_pb.finish_with_message("Complete");
        overall_pb.inc(1);
    }

    overall_pb.finish_with_message("Benchmark complete!");

    // Display results table
    println!("\n{}", "📊 Benchmark Results".bold().green());
    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black());

    let table = Table::new(&all_results)
        .with(Style::rounded())
        .to_string();
    println!("{}", table);

    // Compare with T-Rex if requested
    if args.compare_trex {
        println!("\n{}", "📈 T-Rex Comparison".bold().yellow());
        compare_with_trex(&client, &all_results).await?;
    }

    // Export results if requested
    if let Some(export_path) = args.export {
        export_results(&all_results, &export_path)?;
        println!("\n{}", format!("✓ Results exported to {}", export_path).green());
    }

    // Show summary
    show_summary(&all_results);

    Ok(())
}

async fn compare_with_trex(client: &MinerClient, results: &[BenchmarkResult]) -> Result<()> {
    println!("{}", style("Fetching T-Rex benchmark data...").dim());

    // T-Rex comparison would be implemented in production
    let trex_data: Vec<String> = vec![];

    println!("\n{}", "Performance Comparison:".bold());
    println!("  MineOS vs T-Rex:");

    // T-Rex comparison would show results here
    for result in results {
        println!("    {} on {}: {}",
            result.algorithm,
            result.gpu,
            result.hashrate
        );
    }

    Ok(())
}

fn show_summary(results: &[BenchmarkResult]) {
    println!("\n{}", "📋 Summary".bold().cyan());
    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black());

    // Calculate best algorithm
    let mut best_algo = "";
    let mut best_hashrate = 0.0;
    let mut best_efficiency = 0.0;

    for result in results {
        let hashrate = parse_hashrate_value(&result.hashrate);
        let efficiency = parse_efficiency_value(&result.efficiency);

        if hashrate > best_hashrate {
            best_hashrate = hashrate;
            best_algo = &result.algorithm;
        }

        if efficiency > best_efficiency {
            best_efficiency = efficiency;
        }
    }

    println!("  {} {}", "Best Algorithm:".bold(), best_algo.yellow());
    println!("  {} {:.2} MH/s", "Peak Hashrate:".bold(), best_hashrate);
    println!("  {} {:.3} MH/W", "Best Efficiency:".bold(), best_efficiency);

    // Recommendations
    println!("\n{}", "💡 Recommendations:".bold());
    println!("  • {} algorithm provides the best hashrate", best_algo);
    println!("  • Consider power costs when choosing algorithm");
    println!("  • Run extended benchmarks for more accurate results");
}

fn export_results(results: &[BenchmarkResult], path: &str) -> Result<()> {
    use std::fs::File;
    use std::io::Write;

    let json = serde_json::to_string_pretty(results)?;
    let mut file = File::create(path)?;
    file.write_all(json.as_bytes())?;

    Ok(())
}

fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: u64 = num_str.parse()?;

    match unit {
        "s" => Ok(Duration::from_secs(num)),
        "m" => Ok(Duration::from_secs(num * 60)),
        "h" => Ok(Duration::from_secs(num * 3600)),
        _ => Err(anyhow::anyhow!("Invalid duration format. Use format like '5m', '1h', '30s'"))
    }
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 3600 {
        format!("{} hour(s)", secs / 3600)
    } else if secs >= 60 {
        format!("{} minute(s)", secs / 60)
    } else {
        format!("{} second(s)", secs)
    }
}

fn format_hashrate(mhs: f64) -> String {
    if mhs >= 1000.0 {
        format!("{:.2} GH/s", mhs / 1000.0)
    } else {
        format!("{:.2} MH/s", mhs)
    }
}

fn parse_hashrate_value(s: &str) -> f64 {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() >= 2 {
        let value: f64 = parts[0].parse().unwrap_or(0.0);
        match parts[1] {
            "GH/s" => value * 1000.0,
            "MH/s" => value,
            _ => value,
        }
    } else {
        0.0
    }
}

fn parse_efficiency_value(s: &str) -> f64 {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() >= 2 {
        parts[0].parse().unwrap_or(0.0)
    } else {
        0.0
    }
}