use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber;

mod commands;
mod config;
mod dashboard;
mod utils;
mod client;
mod miner_service;

use commands::*;

/// MineOS - Professional GPU Mining Engine
#[derive(Parser)]
#[command(name = "mineos")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Verbose mode (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Config file path
    #[arg(short, long, global = true)]
    config: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start mining
    Start(start::StartArgs),

    /// Stop mining
    Stop(stop::StopArgs),

    /// Show mining status
    Status(status::StatusArgs),

    /// Run benchmarks
    Benchmark(benchmark::BenchmarkArgs),

    /// Interactive setup wizard
    Setup(setup::SetupArgs),

    /// Configuration management
    Config(config_cmd::ConfigArgs),

    /// GPU overclocking control
    Overclock(overclock::OverclockArgs),

    /// Switch mining algorithm
    Switch(switch::SwitchArgs),

    /// Calculate profitability
    Profit(profit::ProfitArgs),

    /// Open real-time monitoring dashboard
    Dashboard(dashboard::DashboardArgs),

    /// Check for updates
    Update(update::UpdateArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Show banner for interactive commands
    match &cli.command {
        Commands::Setup(_) | Commands::Dashboard(_) => {
            utils::banner::show_animated_banner();
        }
        Commands::Start(_) | Commands::Benchmark(_) => {
            utils::banner::show_banner();
        }
        _ => {
            utils::banner::show_compact_banner();
        }
    }

    // Initialize logging
    let log_level = match cli.verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };

    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .with_target(false)
        .with_thread_ids(false)
        .with_line_number(cli.verbose > 1)
        .init();

    // Load config if specified
    if let Some(config_path) = &cli.config {
        config::set_config_path(config_path);
    }

    // Execute command
    match cli.command {
        Commands::Start(args) => start::execute(args).await?,
        Commands::Stop(args) => stop::execute(args).await?,
        Commands::Status(args) => status::execute(args).await?,
        Commands::Benchmark(args) => benchmark::execute(args).await?,
        Commands::Setup(args) => setup::execute(args).await?,
        Commands::Config(args) => config_cmd::execute(args).await?,
        Commands::Overclock(args) => overclock::execute(args).await?,
        Commands::Switch(args) => switch::execute(args).await?,
        Commands::Profit(args) => profit::execute(args).await?,
        Commands::Dashboard(args) => dashboard::execute(args).await?,
        Commands::Update(args) => update::execute(args).await?,
    }

    Ok(())
}