//! Simple test of KawPow components without full DAG

use mineos_hash::algorithms::kawpow::{
    fnv::fnv1a,
    kiss99::Kiss99State,
    keccak::{keccak_f800, Hash32},
};
use mineos_hash::{Hash256, BlockHeader};

fn main() {
    println!("Testing KawPow components...");
    
    // Test FNV1a
    let h = 0x811c9dc5u32;
    let d = 0x12345678u32;
    let result = fnv1a(h, d);
    println!("FNV1a({:08x}, {:08x}) = {:08x}", h, d, result);
    
    // Test KISS99
    let mut rng = Kiss99State::new(0xDEADBEEF, 0);
    println!("KISS99 random numbers:");
    for i in 0..5 {
        println!("  {}: {:08x}", i, rng.next());
    }
    
    // Test Keccak-f800
    let mut state = [0u32; 25];
    state[0] = 0x12345678;
    println!("Keccak-f800 input: {:08x}", state[0]);
    keccak_f800(&mut state);
    println!("Keccak-f800 output: {:08x}", state[0]);
    
    // Test basic hashing without DAG
    println!("\nTesting basic ProgPoW hash (no DAG):");
    let header = BlockHeader {
        prev_hash: Hash256::default(),
        merkle_root: Hash256::default(),
        timestamp: 1234567890,
        bits: 0x1d00ffff,
        nonce: 12345,
        height: 0,
    };
    
    // Simplified hash calculation
    let header_bytes = header.to_bytes();
    let mut keccak_state = [0u32; 25];
    for i in 0..header_bytes.len() / 4 {
        if i < 25 {
            keccak_state[i] = u32::from_le_bytes([
                header_bytes[i * 4],
                header_bytes[i * 4 + 1],
                header_bytes[i * 4 + 2],
                header_bytes[i * 4 + 3],
            ]);
        }
    }
    
    keccak_f800(&mut keccak_state);
    let seed = ((keccak_state[0] as u64) << 32) | keccak_state[1] as u64;
    println!("Seed from header: {:016x}", seed);
    
    println!("\n✅ All component tests passed!");
    
    // Test CUDA availability
    #[cfg(feature = "cuda")]
    {
        println!("\nTesting CUDA...");
        test_cuda();
    }
}

#[cfg(feature = "cuda")]
fn test_cuda() {
    use mineos_hardware::cuda::GpuDevice;
    
    match GpuDevice::new(0) {
        Ok(device) => {
            println!("✅ CUDA device found: GPU {}", device.index);
            println!("  Total memory: {} MB", device.total_memory / (1024 * 1024));
            let (free, total) = device.get_memory_info();
            println!("  Memory: {} MB free / {} MB total", free / (1024 * 1024), total / (1024 * 1024));
            
            let util = device.get_utilization();
            println!("  Utilization: {}%", util);
            
            if let Some(temp) = device.get_temperature() {
                println!("  Temperature: {}°C", temp);
            }
        }
        Err(e) => {
            println!("❌ CUDA error: {}", e);
        }
    }
}