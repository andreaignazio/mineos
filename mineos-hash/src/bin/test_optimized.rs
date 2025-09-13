//! Test optimized KawPow GPU mining

use mineos_hash::{KawPowCudaMinerOptimized, BlockHeader, Difficulty};
use mineos_hash::algorithms::kawpow::dag::{DagCache, Dag};
use std::time::Instant;
use anyhow::Result;

fn main() -> Result<()> {
    println!("=== Optimized KawPow GPU Mining Test ===\n");
    
    // Initialize optimized CUDA miner
    println!("Initializing OPTIMIZED CUDA miner...");
    let mut miner = KawPowCudaMinerOptimized::new(0)?;
    
    // Get GPU info
    let (free_mem, total_mem) = miner.memory_info()?;
    let utilization = miner.utilization()?;
    
    println!("GPU Status:");
    println!("  Memory: {} MB free / {} MB total", free_mem / 1048576, total_mem / 1048576);
    println!("  Utilization: {:.1}%\n", utilization);
    
    // Test with different DAG sizes
    test_with_dag_size(&mut miner, 0, "Test DAG (1GB)")?;
    
    // Test with production DAG if enough memory
    if free_mem > 4_000_000_000 {
        test_with_dag_size(&mut miner, 333, "Production DAG (3.6GB)")?;
    } else {
        println!("Skipping production DAG test (insufficient memory)\n");
    }
    
    Ok(())
}

fn test_with_dag_size(miner: &mut KawPowCudaMinerOptimized, epoch: u64, label: &str) -> Result<()> {
    println!("Testing with {}", label);
    println!("==================================");
    
    // Generate DAG
    println!("Generating DAG for epoch {}...", epoch);
    let cache_start = Instant::now();
    let cache = DagCache::new(epoch);
    println!("Cache generated in {:?}", cache_start.elapsed());
    
    let dag_start = Instant::now();
    let dag = if epoch == 0 {
        Dag::test_dag(epoch)
    } else {
        println!("Generating full DAG (this will take 3-4 minutes)...");
        Dag::from_cache(&cache)
    };
    println!("DAG generated in {:?}", dag_start.elapsed());
    println!("DAG size: {} MB\n", dag.size / (1024 * 1024));
    
    // Upload DAG to GPU
    println!("Uploading DAG to GPU...");
    let upload_start = Instant::now();
    miner.upload_dag(&dag)?;
    println!("Upload completed in {:?}\n", upload_start.elapsed());
    
    // Test different difficulty levels
    let difficulties = vec![
        ("Easy", 0x207fffff),
        ("Medium", 0x1f7fffff),
        ("Hard", 0x1e7fffff),
        ("Very Hard", 0x1d7fffff),
    ];
    
    for (label, bits) in difficulties {
        println!("Testing {} difficulty (bits: 0x{:08x})", label, bits);
        
        let header = BlockHeader::test_header(epoch * 7500);
        let target = Difficulty::bits_to_target(bits);
        
        // Warm up
        println!("  Warming up...");
        for _ in 0..3 {
            miner.search(&header, &target, 0, 65536)?;
        }
        
        // Benchmark
        println!("  Benchmarking...");
        let mut total_hashes = 0u64;
        let mut solutions_found = 0u32;
        let bench_start = Instant::now();
        let target_duration = std::time::Duration::from_secs(10);
        
        while bench_start.elapsed() < target_duration {
            let batch_size = 1_048_576u32; // 1M hashes per batch
            let result = miner.search(&header, &target, total_hashes, batch_size)?;
            
            if result.is_some() {
                solutions_found += 1;
            }
            
            total_hashes += batch_size as u64;
            
            // Print progress
            if total_hashes % (10 * batch_size as u64) == 0 {
                let elapsed = bench_start.elapsed().as_secs_f64();
                let hashrate = total_hashes as f64 / elapsed;
                print!("\r  Progress: {:.2} MH/s", hashrate / 1_000_000.0);
                use std::io::{self, Write};
                io::stdout().flush()?;
            }
        }
        
        let total_time = bench_start.elapsed();
        let hashrate = total_hashes as f64 / total_time.as_secs_f64();
        
        println!("\n  Results:");
        println!("    Total hashes: {}", total_hashes);
        println!("    Time: {:?}", total_time);
        println!("    Hashrate: {:.2} MH/s", hashrate / 1_000_000.0);
        println!("    Solutions found: {}\n", solutions_found);
    }
    
    // Memory stats after mining
    let (free_mem, total_mem) = miner.memory_info()?;
    let utilization = miner.utilization()?;
    
    println!("GPU Status After Mining:");
    println!("  Memory: {} MB free / {} MB total", free_mem / 1048576, total_mem / 1048576);
    println!("  Utilization: {:.1}%\n", utilization);
    
    Ok(())
}

/// Analyze performance bottlenecks
fn analyze_performance(hashrate: f64) {
    println!("\nPerformance Analysis:");
    println!("=====================");
    
    let target_hashrate = 22_000_000.0; // 22 MH/s for RTX 3060
    let percentage = (hashrate / target_hashrate) * 100.0;
    
    println!("Current: {:.2} MH/s", hashrate / 1_000_000.0);
    println!("Target: {:.2} MH/s (T-Rex on RTX 3060)", target_hashrate / 1_000_000.0);
    println!("Achievement: {:.1}%", percentage);
    
    if percentage < 95.0 {
        println!("\nOptimization suggestions:");
        
        if percentage < 10.0 {
            println!("  - Major architectural issues detected");
            println!("  - Check kernel compilation and launch");
            println!("  - Verify DAG is properly loaded");
            println!("  - Ensure texture memory is being used");
        } else if percentage < 30.0 {
            println!("  - Memory bandwidth likely bottleneck");
            println!("  - Improve memory coalescing");
            println!("  - Increase cache utilization");
            println!("  - Consider texture memory binding");
        } else if percentage < 60.0 {
            println!("  - Compute efficiency needs improvement");
            println!("  - Optimize Keccak implementation");
            println!("  - Use more warp shuffle operations");
            println!("  - Increase instruction-level parallelism");
        } else {
            println!("  - Fine-tuning required");
            println!("  - Adjust grid/block dimensions");
            println!("  - Optimize register usage");
            println!("  - Profile with Nsight Compute");
        }
    } else {
        println!("\n✅ Target performance achieved!");
    }
}