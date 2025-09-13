use anyhow::Result;
use clap::Args;
use colored::*;
use console::style;
use tabled::{Table, Tabled, settings::Style};
use humansize::{format_size, BINARY};
use std::time::Duration;

use crate::client::MinerClient;
use crate::dashboard::widgets::{MinerStatus, GpuStats};

/// Show current mining status
#[derive(Args)]
pub struct StatusArgs {
    /// Output in JSON format
    #[arg(short, long)]
    json: bool,

    /// Watch mode - refresh every N seconds
    #[arg(short, long)]
    watch: Option<u64>,

    /// Show detailed GPU information
    #[arg(short, long)]
    detailed: bool,

    /// Show only specific GPU
    #[arg(short, long)]
    gpu: Option<usize>,
}

#[derive(Tabled)]
struct GpuRow {
    #[tabled(rename = "GPU")]
    index: String,
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Hashrate")]
    hashrate: String,
    #[tabled(rename = "Temp")]
    temperature: String,
    #[tabled(rename = "Power")]
    power: String,
    #[tabled(rename = "Fan")]
    fan: String,
    #[tabled(rename = "Memory")]
    memory: String,
    #[tabled(rename = "Status")]
    status: String,
}

pub async fn execute(args: StatusArgs) -> Result<()> {
    loop {
        // Clear screen in watch mode
        if args.watch.is_some() {
            print!("\x1B[2J\x1B[1;1H");
        }

        // Try to connect to miner
        let client = match MinerClient::connect().await {
            Ok(client) => client,
            Err(_) => {
                show_miner_not_running()?;
                if args.watch.is_some() {
                    tokio::time::sleep(Duration::from_secs(args.watch.unwrap())).await;
                    continue;
                }
                return Ok(());
            }
        };

        // Get status
        let status = client.get_status().await?;
        let gpu_stats = client.get_gpu_statistics().await?;

        if args.json {
            // JSON output
            let json = serde_json::to_string_pretty(&status)?;
            println!("{}", json);
        } else {
            // Formatted output
            show_header(&status)?;
            show_gpu_table(&gpu_stats, args.gpu, args.detailed)?;
            show_summary(&status)?;

            if args.detailed {
                show_detailed_info(&status).await?;
            }
        }

        // Break if not in watch mode
        if let Some(interval) = args.watch {
            tokio::time::sleep(Duration::from_secs(interval)).await;
        } else {
            break;
        }
    }

    Ok(())
}

fn show_miner_not_running() -> Result<()> {
    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black());
    println!("{}", "⚠ MineOS Miner is not running".yellow().bold());
    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black());
    println!();
    println!("Start mining with:");
    println!("  {} {}", style("$").dim(), "mineos start".bold());
    println!("  {} {}", style("$").dim(), "mineos setup".bold());
    Ok(())
}

fn show_header(status: &MinerStatus) -> Result<()> {
    use crate::utils::banner;

    banner::show_divider();
    println!("{} {} | {} | {}",
        "MineOS".bold().cyan(),
        style(env!("CARGO_PKG_VERSION")).dim(),
        status.algorithm.yellow(),
        format_uptime(status.uptime_seconds)
    );
    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black());

    // Pool status
    let pool_status = match status.pool_connected {
        true => "Connected ✓".green(),
        false => "Disconnected ✗".red(),
    };
    println!("  {} {}",
        "Pool:".bold(),
        pool_status
    );

    // Share info
    println!("  {} {} / {}",
        "Shares:".bold(),
        status.accepted_shares.to_string().green(),
        (status.accepted_shares + status.rejected_shares).to_string()
    );

    println!();
    Ok(())
}

fn show_gpu_table(gpus: &[GpuStats], filter: Option<usize>, detailed: bool) -> Result<()> {
    let mut rows = Vec::new();

    for gpu in gpus {
        if let Some(idx) = filter {
            if gpu.index != idx {
                continue;
            }
        }

        let status_icon = "⛏".to_string(); // All GPUs are mining when active

        let temp_str = format_temperature(gpu.temperature);
        let power_str = format!("{} W", gpu.power_usage);
        let fan_str = format!("{}%", gpu.fan_speed);
        let memory_str = if detailed {
            format!("{}",
                format_size(gpu.memory_usage, BINARY)
            )
        } else {
            format_size(gpu.memory_usage, BINARY)
        };

        rows.push(GpuRow {
            index: format!("{} {}", gpu.index, status_icon),
            name: format!("GPU {}", gpu.index),
            hashrate: format_hashrate(gpu.hashrate / 1_000_000.0),
            temperature: temp_str,
            power: power_str,
            fan: fan_str,
            memory: memory_str,
            status: "Mining".to_string(),
        });
    }

    if !rows.is_empty() {
        let table = Table::new(rows)
            .with(Style::rounded())
            .to_string();
        println!("{}", table);
    }

    Ok(())
}

fn show_summary(status: &MinerStatus) -> Result<()> {
    println!();
    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black());

    // Total hashrate
    let hashrate_color = if status.total_hashrate_mhs > 0.0 {
        format!("{:.2} MH/s", status.total_hashrate_mhs).green()
    } else {
        format!("{:.2} MH/s", status.total_hashrate_mhs).red()
    };

    println!("  {} {}",
        "Total:".bold(),
        hashrate_color
    );

    // Profitability tracking would be added in production

    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black());
    Ok(())
}

async fn show_detailed_info(status: &MinerStatus) -> Result<()> {
    println!("\n{}", "Detailed Information:".bold());

    // Share statistics
    let acceptance_rate = if status.accepted_shares + status.rejected_shares > 0 {
        (status.accepted_shares as f64 / (status.accepted_shares + status.rejected_shares) as f64) * 100.0
    } else {
        0.0
    };

    println!("  Share Acceptance Rate: {:.1}%", acceptance_rate);
    println!("  Total Shares: {}", status.total_shares);
    println!("  Stale Shares: {}", status.stale_shares);

    // Additional info would be available in production

    Ok(())
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

fn format_hashrate(mhs: f64) -> String {
    if mhs >= 1000.0 {
        format!("{:.2} GH/s", mhs / 1000.0)
    } else {
        format!("{:.2} MH/s", mhs)
    }
}

fn format_temperature(temp: u32) -> String {
    let colored = if temp > 85 {
        format!("{}°C", temp).red()
    } else if temp > 75 {
        format!("{}°C", temp).yellow()
    } else {
        format!("{}°C", temp).green()
    };
    colored.to_string()
}

// Status structures (would normally be in client module)
use serde::{Deserialize, Serialize};

// MinerStatus imported from dashboard::widgets
/*
struct MinerStatus {
    pub is_mining: bool,
    pub algorithm: String,
    pub pool_url: String,
    pub pool_connected: bool,
    pub worker_name: String,
    pub uptime_seconds: u64,
    pub total_hashrate_mhs: f64,
    pub total_power_watts: u32,
    pub active_gpus: usize,
    pub shares_accepted: u64,
    pub shares_rejected: u64,
    pub avg_share_time: f64,
    pub network_difficulty: String,
    pub block_height: u64,
    pub daily_revenue: f64,
    pub daily_cost: f64,
    pub cpu_usage: f32,
    pub mem_used: u64,
    pub mem_total: u64,
}
*/

// GpuStats imported from dashboard::widgets
/*
struct GpuStatistics {
    pub index: usize,
    pub name: String,
    pub status: String,
    pub hashrate_mhs: f64,
    pub temperature: u32,
    pub power_watts: u32,
    pub fan_speed: u32,
    pub memory_used: u64,
    pub memory_total: u64,
    pub shares_accepted: u64,
    pub shares_rejected: u64,
    pub errors: u32,
}
*/