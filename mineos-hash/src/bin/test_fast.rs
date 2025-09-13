//! Fast test of KawPow with small test DAG

use mineos_hash::{BlockHeader, Hash256, Difficulty};
use mineos_hash::algorithms::kawpow::{
    dag::Dag,
    progpow::ProgPowContext,
};
use std::sync::Arc;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    println!("KawPow Fast Test with Test DAG\n");
    
    // Create test DAG (1MB instead of 1GB+)
    let epoch = 0;
    let dag = Arc::new(Dag::test_dag(epoch));
    println!("Test DAG created: {} bytes\n", dag.size);
    
    // Create test header
    let header = BlockHeader {
        prev_hash: Hash256::from_hex("000000000000000000000000000000000000000000000000000000000000dead")?,
        merkle_root: Hash256::from_hex("000000000000000000000000000000000000000000000000000000000000beef")?,
        timestamp: 1234567890,
        bits: 0x1e00ffff, // Easier difficulty for testing
        nonce: 0,
        height: 1000,
    };
    
    let target = Difficulty::bits_to_target(header.bits);
    println!("Target: {}", target.to_hex());
    
    // Create ProgPoW context
    let ctx = ProgPowContext::new(dag, header.height);
    
    // Test mining
    println!("Testing mining with test DAG...");
    let start = Instant::now();
    let search_range = 100_000;
    
    let result = ctx.search(&header, &target, 0, search_range);
    let elapsed = start.elapsed();
    
    if let Some(result) = result {
        println!("\n✅ Found valid nonce: {}", result.nonce);
        println!("   Hash: {}", result.hash.to_hex());
        if let Some(mix) = result.mix_hash {
            println!("   Mix:  {}", mix.to_hex());
        }
    } else {
        println!("\n❌ No solution found in range 0-{}", search_range);
    }
    
    let hashrate = search_range as f64 / elapsed.as_secs_f64();
    println!("\nPerformance:");
    println!("  Time: {:?}", elapsed);
    println!("  Hashrate: {:.2} H/s", hashrate);
    
    // Test GPU if available
    #[cfg(feature = "cuda")]
    test_gpu()?;
    
    Ok(())
}

#[cfg(feature = "cuda")]
fn test_gpu() -> anyhow::Result<()> {
    use mineos_hash::KawPowCudaMiner;
    use mineos_hardware::cuda::GpuDevice;
    
    println!("\n=== GPU Test ===");
    
    let device = GpuDevice::new(0)?;
    println!("GPU: {} (Memory: {} MB)", device.index, device.total_memory / (1024 * 1024));
    
    let mut miner = KawPowCudaMiner::new(0)?;
    
    // Create test DAG
    let dag = Dag::test_dag(0);
    miner.upload_dag(&dag)?;
    println!("Test DAG uploaded to GPU");
    
    // Create test header
    let header = BlockHeader {
        prev_hash: Hash256::from_hex("000000000000000000000000000000000000000000000000000000000000dead")?,
        merkle_root: Hash256::from_hex("000000000000000000000000000000000000000000000000000000000000beef")?,
        timestamp: 1234567890,
        bits: 0x1e00ffff,
        nonce: 0,
        height: 1000,
    };
    
    let target = Difficulty::bits_to_target(header.bits);
    
    // Test GPU mining
    println!("Testing GPU mining...");
    let start = Instant::now();
    let num_threads = 65536; // 64K threads
    
    let result = miner.search(&header, &target, 0, num_threads)?;
    let elapsed = start.elapsed();
    
    if let Some(result) = result {
        println!("\n✅ GPU found valid nonce: {}", result.nonce);
        println!("   Hash: {}", result.hash.to_hex());
    } else {
        println!("\n❌ GPU: No solution found");
    }
    
    let gpu_hashrate = num_threads as f64 / elapsed.as_secs_f64();
    println!("\nGPU Performance:");
    println!("  Time: {:?}", elapsed);
    println!("  Hashrate: {:.2} MH/s", gpu_hashrate / 1_000_000.0);
    
    Ok(())
}