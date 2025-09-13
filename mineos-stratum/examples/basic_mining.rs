//! Basic mining example showing how to use the Stratum client
//! 
//! This example connects to a mining pool and receives mining jobs.
//! In a real implementation, you would process these jobs and submit shares.

use mineos_stratum::{PoolConfig, Share, StratumClient, StratumConfig};
use std::time::Duration;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("mineos_stratum=debug")
        .init();
    
    info!("Starting MineOS Stratum example");
    
    // Configure mining pools
    // Note: These are example pools - replace with real pool addresses
    let config = StratumConfig {
        pools: vec![
            PoolConfig {
                name: "primary_pool".to_string(),
                // Example: Slushpool Bitcoin testnet
                url: "stratum+tcp://stratum.slushpool.com:3333".to_string(),
                // Replace with your Bitcoin address and worker name
                username: "your_btc_address.worker1".to_string(),
                password: "x".to_string(), // Most pools use 'x' as password
                priority: 0,
                weight: 1,
                enabled: true,
            },
            PoolConfig {
                name: "backup_pool".to_string(),
                // Example: Another pool as backup
                url: "stratum+tcp://pool.example.com:3333".to_string(),
                username: "your_btc_address.worker1".to_string(),
                password: "x".to_string(),
                priority: 1, // Lower priority (higher number)
                weight: 1,
                enabled: true,
            },
        ],
        // Use priority-based failover
        failover_strategy: mineos_stratum::FailoverStrategy::Priority,
        // Other settings use defaults
        ..Default::default()
    };
    
    // Create Stratum client
    let (client, mut job_rx) = StratumClient::new(config);
    
    // Start the client (connects to pool, subscribes, and authorizes)
    match client.start().await {
        Ok(()) => info!("Successfully connected to mining pool"),
        Err(e) => {
            error!("Failed to connect: {}", e);
            return Err(e.into());
        }
    }
    
    // Spawn a task to handle mining jobs
    tokio::spawn(async move {
        info!("Starting job processor");
        
        while let Some(job) = job_rx.recv().await {
            info!("Received new mining job: {}", job);
            info!("  Job ID: {}", job.job_id);
            info!("  Previous hash: {}", job.prev_hash);
            info!("  Clean jobs: {}", job.clean_jobs);
            
            // In a real miner, you would:
            // 1. Build the block header from job parameters
            // 2. Calculate the merkle root
            // 3. Iterate through nonces to find valid shares
            // 4. Submit shares when found
            
            // Example share submission (with dummy values)
            // In reality, these would be calculated from actual mining
            let share = Share {
                worker_name: "your_btc_address.worker1".to_string(),
                job_id: job.job_id.clone(),
                extra_nonce2: "00000000".to_string(), // Would be incremented
                ntime: job.ntime.clone(), // Could be adjusted within limits
                nonce: "12345678".to_string(), // Found through mining
                version_rolling_mask: None,
            };
            
            // Submit share (commented out to avoid rejected shares with dummy data)
            // To submit shares, you would need to pass a reference to the client
            // if let Err(e) = client.submit_share(share).await {
            //     error!("Failed to submit share: {}", e);
            // }
        }
    });
    
    // Monitor statistics (in a real app, you'd use Arc<StratumClient> for sharing)
    // For demo purposes, we'll just skip the monitoring task
    
    // Keep the program running
    info!("Mining client is running. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;
    
    info!("Shutting down...");
    client.stop().await;
    
    Ok(())
}

// Note: To run this example with a real pool, you need:
// 1. A valid Bitcoin (or other cryptocurrency) address
// 2. Pool credentials (some pools require registration)
// 3. Network connectivity to the pool
// 
// For testing, you can use testnet pools:
// - Testnet Slushpool: stratum+tcp://stratum.slushpool.com:3333
// - Get testnet Bitcoin from a faucet
// 
// Run with: cargo run --example basic_mining