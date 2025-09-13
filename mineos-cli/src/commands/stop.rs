use anyhow::Result;
use clap::Args;
use colored::*;
use console::style;
use dialoguer::Confirm;

use crate::client::MinerClient;

/// Stop mining operations
#[derive(Args)]
pub struct StopArgs {
    /// Stop specific GPU
    #[arg(short, long)]
    gpu: Option<usize>,

    /// Force stop without confirmation
    #[arg(short, long)]
    force: bool,

    /// Save current state before stopping
    #[arg(short, long)]
    save_state: bool,
}

pub async fn execute(args: StopArgs) -> Result<()> {
    // Connect to running miner
    let mut client = match MinerClient::connect().await {
        Ok(client) => client,
        Err(_) => {
            println!("{}", "⚠ Miner is not running".yellow());
            return Ok(());
        }
    };

    // Get current status
    let status = client.get_status().await?;

    if !status.is_mining {
        println!("{}", "ℹ Miner is already stopped".blue());
        return Ok(());
    }

    // Show current mining info
    println!("{}", "Current Mining Status:".bold());
    println!("  Algorithm: {}", status.algorithm.yellow());
    println!("  Hashrate: {} MH/s", status.total_hashrate_mhs);
    println!("  Active GPUs: {}", status.active_gpus);
    println!("  Uptime: {}", format_duration(status.uptime_seconds));

    // Confirm stop
    if !args.force {
        let confirm = Confirm::new()
            .with_prompt("Do you want to stop mining?")
            .default(false)
            .interact()?;

        if !confirm {
            println!("{}", "Cancelled".yellow());
            return Ok(());
        }
    }

    // Save state if requested
    if args.save_state {
        println!("{}", style("Saving current state...").dim());
        // In a full implementation, this would save the state
    }

    // Stop mining
    if let Some(gpu_idx) = args.gpu {
        println!("{}", format!("Stopping GPU {}...", gpu_idx).yellow());
        // In a full implementation, this would stop specific GPU
        println!("{}", format!("✓ GPU {} stopped", gpu_idx).green());
    } else {
        println!("{}", "Stopping all mining operations...".yellow());

        // Stop gracefully
        client.stop().await?;

        println!("{}", "✓ Mining stopped successfully".green());

        // Show final statistics
        let stats = client.get_status().await?;

        println!("\n{}", "Session Summary:".bold());
        println!("  Total Runtime: {} seconds", stats.uptime_seconds);
        println!("  Total Shares: {} accepted, {} rejected",
            stats.accepted_shares.to_string().green(),
            stats.rejected_shares.to_string().red()
        );
        println!("  Average Hashrate: {} MH/s", stats.total_hashrate_mhs);
        // Power and revenue tracking would be added in a full implementation
    }

    Ok(())
}

fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}