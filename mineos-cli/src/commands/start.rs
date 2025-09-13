use anyhow::{Result, Context};
use clap::Args;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

use crate::client::MinerClient;
use crate::config::{load_config, MinerConfig};
use crate::utils::gpu_selector::parse_gpu_list;
use console::style;

/// Start mining with specified configuration
#[derive(Args)]
pub struct StartArgs {
    /// Mining algorithm (e.g., kawpow, ethash, kheavyhash)
    #[arg(short, long)]
    pub algorithm: Option<String>,

    /// Pool URL (e.g., stratum+tcp://pool.example.com:3333)
    #[arg(short, long)]
    pub pool: Option<String>,

    /// Wallet address
    #[arg(short, long)]
    pub wallet: Option<String>,

    /// Worker name
    #[arg(short = 'n', long)]
    pub worker_name: Option<String>,

    /// GPU indices to use (e.g., 0,1,2 or all)
    #[arg(short, long, default_value = "all")]
    pub gpus: String,

    /// Start in benchmark mode
    #[arg(short, long)]
    pub benchmark: bool,

    /// Dry run - validate config without starting
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn execute(args: StartArgs) -> Result<()> {
    use crate::utils::banner;

    // Show mining animation
    banner::show_mining_animation();

    // Load configuration
    let mut config = if args.algorithm.is_some() || args.pool.is_some() {
        // Create config from command line args
        create_config_from_args(&args)?
    } else {
        // Load from config file
        load_config().context("Failed to load configuration. Run 'mineos setup' first.")?
    };

    // Parse GPU selection
    let selected_gpus = parse_gpu_list(&args.gpus)?;
    config.gpus.enabled = selected_gpus.clone();

    // Validate configuration
    validate_config(&config)?;

    if args.dry_run {
        println!("{}", "✓ Configuration validated successfully".green());
        println!("\n{}", "Configuration:".bold());
        println!("  Algorithm: {}", config.algorithm.yellow());
        println!("  Pool: {}", config.pool.url.cyan());
        println!("  Wallet: {}", format_wallet(&config.pool.wallet));
        println!("  Worker: {}", config.worker_name.white());
        println!("  GPUs: {:?}", selected_gpus);
        return Ok(());
    }

    // Initialize progress bar
    let pb = ProgressBar::new(5);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{bar:40.cyan/blue}] {pos}/{len} {msg}")?
            .progress_chars("=>-"),
    );

    // Connect to miner daemon or start it
    pb.set_message("Connecting to miner daemon...");
    let mut client = MinerClient::connect().await?;
    pb.inc(1);

    // Detect hardware
    pb.set_message("Detecting GPUs...");
    let gpu_info = crate::utils::gpu_selector::detect_gpus()?;
    pb.inc(1);
    pb.finish_and_clear();

    // Show fancy GPU detection
    banner::show_gpu_detection_animation(gpu_info.len());
    println!();

    for gpu in &gpu_info {
        println!("  {} GPU {}: {} {}",
            "▸".bright_cyan(),
            gpu.index.to_string().bright_yellow(),
            gpu.name.bright_white().bold(),
            format!("({}GB)", gpu.memory).bright_black()
        );
    }
    println!();

    // Validate selected GPUs exist
    for gpu_idx in &selected_gpus {
        if !gpu_info.iter().any(|g| g.index == *gpu_idx) {
            return Err(anyhow::anyhow!("GPU {} not found", gpu_idx));
        }
    }

    // Start mining with the actual configuration
    pb.set_message("Starting mining threads...");
    client.start_mining(config.clone()).await?;
    pb.inc(3); // Increment for algorithm init, pool connect, and start

    pb.finish_with_message("Mining started successfully!");

    // Print fancy startup summary
    println!();
    banner::show_box("✓ Mining Started Successfully!", vec![
        &format!("Algorithm: {}", config.algorithm),
        &format!("Pool: Connected"),
        &format!("GPUs: {} active", selected_gpus.len()),
        "",
        "Happy Mining! ⛏️"
    ]);
    println!();
    println!("  {} {}", "Algorithm:".bold(), config.algorithm.yellow());
    println!("  {} {}", "Pool:".bold(), config.pool.url.cyan());
    println!("  {} {}", "Worker:".bold(), config.worker_name.white());
    println!("  {} {} GPU(s)", "Active:".bold(), selected_gpus.len());
    println!();
    println!("{}", style("Monitor your mining with:").dim());
    println!("  {} {}", style("$").dim(), "mineos status".bold());
    println!("  {} {}", style("$").dim(), "mineos dashboard".bold());
    println!();

    if args.benchmark {
        println!("{}", "Running in benchmark mode for 5 minutes...".yellow());
        tokio::time::sleep(Duration::from_secs(300)).await;

        let stats = client.get_status().await?;
        println!("\n{}", "Benchmark Results:".bold());
        println!("  Average Hashrate: {} MH/s", stats.total_hashrate / 1_000_000.0);
        println!("  Accepted Shares: {}", stats.accepted_shares);
        println!("  Rejected Shares: {}", stats.rejected_shares);
    }

    Ok(())
}

fn create_config_from_args(args: &StartArgs) -> Result<MinerConfig> {
    let algorithm = args.algorithm.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Algorithm must be specified"))?;

    let pool_url = args.pool.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Pool URL must be specified"))?;

    let wallet = args.wallet.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Wallet address must be specified"))?;

    Ok(MinerConfig {
        algorithm: algorithm.clone(),
        worker_name: args.worker_name.clone()
            .unwrap_or_else(|| hostname()
                .unwrap_or_else(|_| "mineos".to_string())),
        pool: crate::config::PoolConfig {
            url: pool_url.clone(),
            wallet: wallet.clone(),
            password: "x".to_string(),
        },
        gpus: crate::config::GpuConfig {
            enabled: vec![],
            overclocks: vec![],
        },
        monitoring: crate::config::MonitoringConfig {
            update_interval: 1000,
            temperature_limit: 85,
        },
        profit_switching: None,
    })
}

fn validate_config(config: &MinerConfig) -> Result<()> {
    // Validate algorithm
    let valid_algos = ["kawpow", "ethash", "etchash", "kheavyhash", "autolykos2"];
    if !valid_algos.contains(&config.algorithm.as_str()) {
        return Err(anyhow::anyhow!(
            "Invalid algorithm '{}'. Valid options: {:?}",
            config.algorithm,
            valid_algos
        ));
    }

    // Validate pool URL
    if !config.pool.url.starts_with("stratum+tcp://") &&
       !config.pool.url.starts_with("stratum+ssl://") {
        return Err(anyhow::anyhow!(
            "Invalid pool URL. Must start with stratum+tcp:// or stratum+ssl://"
        ));
    }

    // Validate wallet address (basic check)
    if config.pool.wallet.len() < 20 {
        return Err(anyhow::anyhow!("Wallet address seems too short"));
    }

    Ok(())
}

fn format_wallet(wallet: &str) -> String {
    if wallet.len() > 20 {
        format!("{}...{}", &wallet[..8], &wallet[wallet.len()-8..])
    } else {
        wallet.to_string()
    }
}

fn hostname() -> Result<String> {
    Ok(gethostname::gethostname()
        .to_string_lossy()
        .to_string())
}

use once_cell::sync::Lazy;
static HOSTNAME: Lazy<String> = Lazy::new(|| {
    gethostname::gethostname()
        .to_string_lossy()
        .to_string()
});