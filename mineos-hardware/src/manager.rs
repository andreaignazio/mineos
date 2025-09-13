use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};
use cudarc::driver::PushKernelArg;

use crate::cuda::{GpuDevice, KernelManager, MemoryPool, Result};
use crate::detection::{detect_cuda_devices, CudaInfo, GpuDeviceInfo};
use crate::monitor::{GpuMonitor, GpuMetrics};

/// High-level GPU manager that coordinates all GPU operations
pub struct GpuManager {
    /// Information about CUDA environment
    pub cuda_info: CudaInfo,
    
    /// GPU devices indexed by their ID
    devices: RwLock<HashMap<usize, Arc<GpuDevice>>>,
    
    /// Kernel managers for each device
    kernel_managers: RwLock<HashMap<usize, Arc<KernelManager>>>,
    
    /// Memory pools for each device
    memory_pools: RwLock<HashMap<usize, Arc<MemoryPool>>>,
    
    /// GPU monitor (if available)
    monitor: Option<Arc<GpuMonitor>>,
}

impl GpuManager {
    /// Create a new GPU manager and initialize all devices
    pub fn new() -> Result<Self> {
        info!("Initializing GPU manager");
        
        // Detect CUDA devices
        let cuda_info = detect_cuda_devices()?;
        
        // Initialize devices
        let mut devices = HashMap::new();
        let mut kernel_managers = HashMap::new();
        let mut memory_pools = HashMap::new();
        
        for device_info in &cuda_info.devices {
            let idx = device_info.index;
            
            // Create device
            match GpuDevice::new(idx) {
                Ok(device) => {
                    let device = Arc::new(device);
                    
                    // Create kernel manager
                    let kernel_mgr = Arc::new(KernelManager::new(device.context().clone(), device.stream().clone()));
                    
                    // Load test kernels
                    if let Err(e) = kernel_mgr.load_test_kernel() {
                        warn!("Failed to load test kernels for GPU {}: {}", idx, e);
                    }
                    
                    // Create memory pool (1GB max per GPU)
                    let pool_size = 1024 * 1024 * 1024; // 1GB
                    let memory_pool = Arc::new(MemoryPool::new(
                        device.stream().clone(),
                        pool_size,
                    ));
                    
                    devices.insert(idx, device);
                    kernel_managers.insert(idx, kernel_mgr);
                    memory_pools.insert(idx, memory_pool);
                }
                Err(e) => {
                    warn!("Failed to initialize GPU {}: {}", idx, e);
                }
            }
        }
        
        // Initialize monitor (optional, may fail if NVML not available)
        let monitor = match GpuMonitor::new() {
            Ok(m) => {
                info!("GPU monitoring enabled via NVML");
                Some(Arc::new(m))
            }
            Err(e) => {
                warn!("GPU monitoring not available: {}", e);
                None
            }
        };
        
        Ok(Self {
            cuda_info,
            devices: RwLock::new(devices),
            kernel_managers: RwLock::new(kernel_managers),
            memory_pools: RwLock::new(memory_pools),
            monitor,
        })
    }
    
    /// Get the number of available GPUs
    pub fn device_count(&self) -> usize {
        self.cuda_info.device_count
    }
    
    /// Get information about all devices
    pub fn device_info(&self) -> &[GpuDeviceInfo] {
        &self.cuda_info.devices
    }
    
    /// Get a specific GPU device
    pub fn get_device(&self, index: usize) -> Option<Arc<GpuDevice>> {
        let devices = self.devices.read().unwrap();
        devices.get(&index).cloned()
    }
    
    /// Get the kernel manager for a device
    pub fn get_kernel_manager(&self, index: usize) -> Option<Arc<KernelManager>> {
        let managers = self.kernel_managers.read().unwrap();
        managers.get(&index).cloned()
    }
    
    /// Get the memory pool for a device
    pub fn get_memory_pool(&self, index: usize) -> Option<Arc<MemoryPool>> {
        let pools = self.memory_pools.read().unwrap();
        pools.get(&index).cloned()
    }
    
    /// Get current metrics for all GPUs
    pub fn get_metrics(&self) -> Vec<GpuMetrics> {
        match &self.monitor {
            Some(m) => m.get_all_metrics(),
            None => Vec::new(),
        }
    }
    
    /// Get metrics for a specific GPU
    pub fn get_device_metrics(&self, index: usize) -> Option<GpuMetrics> {
        match &self.monitor {
            Some(m) => m.get_metrics(index).ok(),
            None => None,
        }
    }
    
    /// Select the best GPU based on available memory
    pub fn select_best_gpu(&self) -> Option<usize> {
        let devices = self.devices.read().unwrap();
        
        let mut best_gpu = None;
        let mut max_memory = 0;
        
        for (&idx, device) in devices.iter() {
            if let Ok(available) = device.available_memory() {
                if available > max_memory {
                    max_memory = available;
                    best_gpu = Some(idx);
                }
            }
        }
        
        if let Some(idx) = best_gpu {
            info!("Selected GPU {} with {} MB available memory", 
                  idx, max_memory / 1024 / 1024);
        }
        
        best_gpu
    }
    
    /// Run a simple benchmark on all GPUs
    pub async fn benchmark_all(&self) -> HashMap<usize, BenchmarkResult> {
        let mut results = HashMap::new();
        
        for device_info in &self.cuda_info.devices {
            let idx = device_info.index;
            
            if let Some(result) = self.benchmark_device(idx).await {
                results.insert(idx, result);
            }
        }
        
        results
    }
    
    /// Benchmark a specific device
    async fn benchmark_device(&self, index: usize) -> Option<BenchmarkResult> {
        let device = self.get_device(index)?;
        let kernel_mgr = self.get_kernel_manager(index)?;
        let memory_pool = self.get_memory_pool(index)?;
        
        // Allocate test buffers (100MB)
        let size = 100 * 1024 * 1024;
        let elements = size / 4; // float32 elements
        
        let mut buffer_a = memory_pool.allocate(size).ok()?;
        let mut buffer_b = memory_pool.allocate(size).ok()?;
        let mut buffer_c = memory_pool.allocate(size).ok()?;
        
        // Initialize with test data
        let data = vec![1.0f32; elements];
        let bytes = unsafe {
            std::slice::from_raw_parts(data.as_ptr() as *const u8, size)
        };
        
        buffer_a.copy_from_host(bytes).ok()?;
        buffer_b.copy_from_host(bytes).ok()?;
        
        // Get kernel
        let kernel = kernel_mgr.get_function("test_kernels", "vector_add").ok()?;
        
        // Configure launch
        let config = crate::cuda::KernelLauncher::for_num_elems(elements as u32);
        
        // Measure time
        let start = std::time::Instant::now();
        
        // Launch kernel multiple times for better measurement
        for _ in 0..100 {
            // Use stream's launch_builder directly as shown in API guide
            unsafe {
                device.stream()
                    .launch_builder(&kernel)
                    .arg(buffer_a.as_device_slice_mut())
                    .arg(buffer_b.as_device_slice())
                    .arg(buffer_c.as_device_slice_mut())
                    .arg(&(elements as i32))
                    .launch(config).ok()?;
            }
        }
        
        device.synchronize().ok()?;
        let elapsed = start.elapsed();
        
        // Calculate throughput
        let total_bytes = size * 3 * 100; // 3 buffers, 100 iterations
        let throughput_gbps = (total_bytes as f64) / elapsed.as_secs_f64() / 1e9;
        
        Some(BenchmarkResult {
            device_index: index,
            memory_bandwidth_gbps: throughput_gbps,
            kernel_time_ms: elapsed.as_millis() as f32 / 100.0,
        })
    }
    
    /// Synchronize all devices
    pub fn synchronize_all(&self) -> Result<()> {
        let devices = self.devices.read().unwrap();
        
        for device in devices.values() {
            device.synchronize()?;
        }
        
        Ok(())
    }
    
    /// Get total memory across all GPUs
    pub fn total_memory(&self) -> usize {
        self.cuda_info.devices.iter()
            .map(|d| d.total_memory_mb * 1024 * 1024)
            .sum()
    }
}

/// Result of a GPU benchmark
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub device_index: usize,
    pub memory_bandwidth_gbps: f64,
    pub kernel_time_ms: f32,
}