use cudarc::driver::CudaContext;
use tracing::{debug, info, warn};

use crate::cuda::device::GpuDevice;
use crate::cuda::error::{GpuError, Result};

/// Information about the CUDA environment
#[derive(Debug, Clone)]
pub struct CudaInfo {
    /// CUDA driver version
    pub driver_version: String,
    
    /// Number of CUDA devices
    pub device_count: usize,
    
    /// List of available devices
    pub devices: Vec<GpuDeviceInfo>,
}

/// Basic information about a GPU device
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GpuDeviceInfo {
    pub index: usize,
    pub name: String,
    pub total_memory_mb: usize,
    pub compute_capability: (u32, u32),
    pub multiprocessor_count: u32,
    pub clock_rate_mhz: u32,
}

impl From<&GpuDevice> for GpuDeviceInfo {
    fn from(device: &GpuDevice) -> Self {
        Self {
            index: device.index,
            name: device.name.clone(),
            total_memory_mb: device.total_memory / 1024 / 1024,
            compute_capability: device.compute_capability,
            multiprocessor_count: device.multiprocessor_count,
            clock_rate_mhz: device.clock_rate_mhz,
        }
    }
}

/// Detect all available CUDA devices
pub fn detect_cuda_devices() -> Result<CudaInfo> {
    info!("Detecting CUDA devices...");
    
    // Get driver version
    let driver_version = get_driver_version()?;
    info!("CUDA driver version: {}", driver_version);
    
    // Get device count using the static method
    let device_count = CudaContext::device_count()? as usize;
    
    if device_count == 0 {
        warn!("No CUDA devices found");
        return Err(GpuError::NoDevicesFound);
    }
    
    info!("Found {} CUDA device(s)", device_count);
    
    // Enumerate devices
    let mut devices = Vec::new();
    for i in 0..device_count {
        match GpuDevice::new(i) {
            Ok(device) => {
                let info = GpuDeviceInfo::from(&device);
                debug!("GPU {}: {} ({}MB, CC {}.{})", 
                       i, info.name, info.total_memory_mb, 
                       info.compute_capability.0, info.compute_capability.1);
                devices.push(info);
            }
            Err(e) => {
                warn!("Failed to initialize GPU {}: {}", i, e);
            }
        }
    }
    
    if devices.is_empty() {
        return Err(GpuError::NoDevicesFound);
    }
    
    Ok(CudaInfo {
        driver_version,
        device_count,
        devices,
    })
}

/// Get CUDA driver version as a string
fn get_driver_version() -> Result<String> {
    // Try to create a context to check driver
    match CudaContext::new(0) {
        Ok(ctx) => {
            // Get device to access driver info
            // cu_device() returns an i32, not a Result
            let _device_id = ctx.cu_device();
            // For now, return a placeholder since cudarc doesn't expose driver version directly
            Ok("12.x".to_string())
        }
        Err(_) => Err(GpuError::NoDevicesFound)
    }
}

/// Check if CUDA is available on this system
pub fn is_cuda_available() -> bool {
    // Try to create a context for device 0
    CudaContext::new(0).is_ok()
}

/// Get optimal thread block size for a given device
pub fn get_optimal_block_size(device: &GpuDevice) -> usize {
    // Generally, 256 threads per block is a good default
    // Can be tuned based on specific kernels
    match device.compute_capability {
        (major, _) if major >= 8 => 256,  // RTX 30/40 series
        (major, _) if major >= 7 => 256,  // RTX 20 series, Tesla V100
        (major, _) if major >= 6 => 128,  // GTX 10 series
        _ => 64,  // Older GPUs
    }
}

/// Calculate optimal grid size for given problem size
pub fn calculate_grid_size(problem_size: usize, block_size: usize) -> usize {
    (problem_size + block_size - 1) / block_size
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cuda_detection() {
        // This test will only pass on systems with CUDA
        if is_cuda_available() {
            let info = detect_cuda_devices().unwrap();
            assert!(info.device_count > 0);
            assert!(!info.devices.is_empty());
            
            // Check first device
            let first = &info.devices[0];
            assert!(!first.name.is_empty());
            assert!(first.total_memory_mb > 0);
        } else {
            println!("CUDA not available, skipping test");
        }
    }
    
    #[test]
    fn test_optimal_block_size() {
        // Mock device for testing
        if is_cuda_available() {
            if let Ok(device) = GpuDevice::new(0) {
                let block_size = get_optimal_block_size(&device);
                assert!(block_size > 0);
                assert!(block_size <= 1024); // Max threads per block
            }
        }
    }
    
    #[test]
    fn test_grid_size_calculation() {
        assert_eq!(calculate_grid_size(1000, 256), 4);
        assert_eq!(calculate_grid_size(1024, 256), 4);
        assert_eq!(calculate_grid_size(1025, 256), 5);
    }
}