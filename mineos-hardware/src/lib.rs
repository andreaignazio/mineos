// Re-export main components for easy access
pub use cuda::{GpuDevice, GpuError, KernelManager, MemoryPool, Result};
pub use detection::{detect_cuda_devices, is_cuda_available, CudaInfo, GpuDeviceInfo};
pub use manager::{GpuManager, BenchmarkResult};
pub use monitor::{GpuMonitor, GpuMetrics, MetricsCollector};

pub mod cuda;
pub mod detection;
pub mod manager;
pub mod monitor;

/// Version of the mineos-hardware library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Get library version
pub fn version() -> &'static str {
    VERSION
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_version() {
        assert!(!version().is_empty());
    }
    
    #[test]
    fn test_cuda_availability() {
        // This will return true/false based on system
        let available = is_cuda_available();
        println!("CUDA available: {}", available);
    }
    
    #[tokio::test]
    async fn test_gpu_manager() {
        match GpuManager::new() {
            Ok(manager) => {
                println!("Found {} GPU(s)", manager.device_count());
                
                for device_info in manager.device_info() {
                    println!("GPU {}: {} ({}MB, CC {}.{})",
                             device_info.index,
                             device_info.name,
                             device_info.total_memory_mb,
                             device_info.compute_capability.0,
                             device_info.compute_capability.1);
                }
                
                // Try to get metrics
                let metrics = manager.get_metrics();
                for m in metrics {
                    println!("GPU {} metrics: {}°C, {}W, {}% utilization",
                             m.index, m.temperature, m.power_usage, m.gpu_utilization);
                }
                
                // Run benchmark
                println!("Running benchmark...");
                let results = manager.benchmark_all().await;
                for (idx, result) in results {
                    println!("GPU {} benchmark: {:.2} GB/s bandwidth, {:.2}ms kernel",
                             idx, result.memory_bandwidth_gbps, result.kernel_time_ms);
                }
            }
            Err(e) => {
                println!("Failed to initialize GPU manager: {}", e);
            }
        }
    }
}
