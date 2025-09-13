//! Test KawPow implementation on real GPU

use mineos_hash::{KawPowMiner, BlockHeader, Hash256, Difficulty};
#[cfg(feature = "cuda")]
use mineos_hash::KawPowCudaMiner;
use std::time::Instant;
use tracing::info;
#[cfg(not(feature = "cuda"))]
use tracing::error;

fn main() -> anyhow::Result<()> {
    // Simple println for testing
    println!("Starting KawPow GPU test");
    
    // Test CPU implementation first
    info!("Testing CPU implementation");
    test_cpu_mining()?;
    
    // Check for CUDA devices
    #[cfg(feature = "cuda")]
    {
        info!("\nTesting CUDA implementation");
        test_cuda_mining()?;
    }
    
    Ok(())
}

fn test_cpu_mining() -> anyhow::Result<()> {
    println!("Creating KawPow miner...");
    let miner = KawPowMiner::new(1);
    println!("Miner created");
    
    // Create a test block header (Ravencoin mainnet-like)
    let header = BlockHeader {
        prev_hash: Hash256::from_hex("000000000000000000000000000000000000000000000000000000000000dead")?,
        merkle_root: Hash256::from_hex("000000000000000000000000000000000000000000000000000000000000beef")?,
        timestamp: 1234567890,
        bits: 0x1d00ffff, // Easy difficulty for testing
        nonce: 0,
        height: 1000, // Use early epoch for faster testing (epoch 0)
    };
    
    let target = Difficulty::bits_to_target(header.bits);
    println!("Mining with target: {}", target.to_hex());
    
    println!("Starting mining...");
    let start = Instant::now();
    let result = miner.mine(&header, &target, 0, 1000); // Reduced range for testing
    println!("Mining completed");
    let elapsed = start.elapsed();
    
    if let Some(result) = result {
        info!("Found valid nonce: {} in {:?}", result.nonce, elapsed);
        info!("Hash: {}", result.hash.to_hex());
        if let Some(mix) = result.mix_hash {
            info!("Mix hash: {}", mix.to_hex());
        }
        
        // Calculate hashrate
        let hashes = result.nonce as f64;
        let hashrate = hashes / elapsed.as_secs_f64();
        info!("CPU Hashrate: {:.2} H/s", hashrate);
    } else {
        info!("No solution found in range");
    }
    
    Ok(())
}

#[cfg(feature = "cuda")]
fn test_cuda_mining() -> anyhow::Result<()> {
    use mineos_hardware::cuda::GpuDevice;
    
    // Initialize GPU
    let device = GpuDevice::new(0)?;
    info!("Using GPU: {} ({})", device.name, device.index);
    info!("Total memory: {} MB", device.total_memory / (1024 * 1024));
    
    let (free, total) = device.get_memory_info();
    info!("Memory: {} MB free / {} MB total", free / (1024 * 1024), total / (1024 * 1024));
    
    // Create CUDA miner
    let mut cuda_miner = KawPowCudaMiner::new(0)?;
    
    // Create test header
    let header = BlockHeader {
        prev_hash: Hash256::from_hex("000000000000000000000000000000000000000000000000000000000000dead")?,
        merkle_root: Hash256::from_hex("000000000000000000000000000000000000000000000000000000000000beef")?,
        timestamp: 1234567890,
        bits: 0x1d00ffff,
        nonce: 0,
        height: 1000, // Use epoch 0 for testing
    };
    
    let target = Difficulty::bits_to_target(header.bits);
    
    // Generate and upload DAG
    info!("Generating DAG for epoch {}...", header.height / 7500);
    let dag_start = Instant::now();
    
    use mineos_hash::algorithms::kawpow::dag::{DagCache, Dag};
    let epoch = header.height / 7500;
    let cache = DagCache::new(epoch);
    let dag = Dag::from_cache(&cache);
    
    info!("DAG generated in {:?}", dag_start.elapsed());
    info!("Uploading DAG to GPU...");
    
    cuda_miner.upload_dag(&dag)?;
    info!("DAG uploaded");
    
    // Benchmark GPU mining
    let num_threads = 1024 * 1024; // 1M threads
    let iterations = 10;
    
    info!("Starting GPU benchmark with {} threads, {} iterations", num_threads, iterations);
    
    let mut total_time = std::time::Duration::ZERO;
    let mut solutions_found = 0;
    
    for i in 0..iterations {
        let start_nonce = (i as u64) * (num_threads as u64);
        
        let start = Instant::now();
        let result = cuda_miner.search(&header, &target, start_nonce, num_threads)?;
        let elapsed = start.elapsed();
        
        total_time += elapsed;
        
        if let Some(result) = result {
            solutions_found += 1;
            info!("Iteration {}: Found nonce {} in {:?}", i, result.nonce, elapsed);
        }
    }
    
    // Calculate hashrate
    let total_hashes = (num_threads as u64) * (iterations as u64);
    let hashrate = total_hashes as f64 / total_time.as_secs_f64();
    
    info!("\n=== GPU Mining Results ===");
    info!("Total time: {:?}", total_time);
    info!("Total hashes: {}", total_hashes);
    info!("Solutions found: {}", solutions_found);
    info!("Hashrate: {:.2} MH/s", hashrate / 1_000_000.0);
    
    // Check GPU utilization
    let utilization = device.get_utilization();
    info!("GPU Utilization: {:.1}%", utilization);
    
    if let Some(temp) = device.get_temperature() {
        info!("GPU Temperature: {:.1}°C", temp);
    }
    
    if let Some(power) = device.get_power_usage() {
        info!("Power Usage: {:.1}W", power);
    }
    
    Ok(())
}

#[cfg(not(feature = "cuda"))]
fn test_cuda_mining() -> anyhow::Result<()> {
    error!("CUDA support not enabled. Rebuild with --features cuda");
    Ok(())
}