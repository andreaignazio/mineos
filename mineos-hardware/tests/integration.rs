use mineos_hardware::{GpuManager, is_cuda_available};
use cudarc::driver::PushKernelArg;

#[tokio::test]
async fn test_gpu_detection() {
    if !is_cuda_available() {
        println!("CUDA not available, skipping GPU tests");
        return;
    }
    
    let manager = GpuManager::new().expect("Failed to create GPU manager");
    assert!(manager.device_count() > 0);
    
    // Print device info
    for device in manager.device_info() {
        println!("Found GPU: {} with {}MB memory", device.name, device.total_memory_mb);
    }
}

#[tokio::test]
async fn test_gpu_memory_allocation() {
    if !is_cuda_available() {
        println!("CUDA not available, skipping test");
        return;
    }
    
    let manager = GpuManager::new().expect("Failed to create GPU manager");
    
    if let Some(device_idx) = manager.select_best_gpu() {
        let _device = manager.get_device(device_idx).expect("Failed to get device");
        let pool = manager.get_memory_pool(device_idx).expect("Failed to get memory pool");
        
        // Allocate 100MB
        let size = 100 * 1024 * 1024;
        let buffer = pool.allocate(size).expect("Failed to allocate memory");
        
        assert_eq!(buffer.size(), size);
        
        // Test memory transfer
        let data = vec![42u8; size];
        let mut buffer = buffer;
        buffer.copy_from_host(&data).expect("Failed to copy to GPU");
        
        let result = buffer.copy_to_host().expect("Failed to copy from GPU");
        
        assert_eq!(result[0], 42);
        assert_eq!(result[size - 1], 42);
    }
}

#[tokio::test]
async fn test_kernel_execution() {
    if !is_cuda_available() {
        println!("CUDA not available, skipping test");
        return;
    }
    
    let manager = GpuManager::new().expect("Failed to create GPU manager");
    
    if let Some(device_idx) = manager.select_best_gpu() {
        let device = manager.get_device(device_idx).expect("Failed to get device");
        let kernel_mgr = manager.get_kernel_manager(device_idx).expect("Failed to get kernel manager");
        let pool = manager.get_memory_pool(device_idx).expect("Failed to get memory pool");
        
        // Load test kernel
        kernel_mgr.load_test_kernel().expect("Failed to load test kernel");
        
        // Allocate buffers for vector addition
        let elements = 1024;
        let size = elements * std::mem::size_of::<f32>();
        
        let mut a = pool.allocate(size).expect("Failed to allocate A");
        let mut b = pool.allocate(size).expect("Failed to allocate B");
        let mut c = pool.allocate(size).expect("Failed to allocate C");
        
        // Initialize data
        let data_a: Vec<f32> = (0..elements).map(|i| i as f32).collect();
        let data_b: Vec<f32> = (0..elements).map(|i| (i * 2) as f32).collect();
        
        // Copy to GPU
        let bytes_a = unsafe {
            std::slice::from_raw_parts(data_a.as_ptr() as *const u8, size)
        };
        let bytes_b = unsafe {
            std::slice::from_raw_parts(data_b.as_ptr() as *const u8, size)
        };
        
        a.copy_from_host(bytes_a).expect("Failed to copy A");
        b.copy_from_host(bytes_b).expect("Failed to copy B");
        
        // Get kernel
        let kernel = kernel_mgr.get_function("test_kernels", "vector_add")
            .expect("Failed to get kernel");
        
        // Launch kernel using the new API
        let config = mineos_hardware::cuda::KernelLauncher::for_num_elems(elements as u32);
        
        unsafe {
            device.stream()
                .launch_builder(&kernel)
                .arg(c.as_device_slice_mut())
                .arg(a.as_device_slice())
                .arg(b.as_device_slice())
                .arg(&(elements as i32))
                .launch(config)
                .expect("Failed to launch kernel");
        }
        
        device.synchronize().expect("Failed to synchronize");
        
        // Get results
        let result = c.copy_to_host().expect("Failed to copy result");
        
        let result_floats = unsafe {
            std::slice::from_raw_parts(result.as_ptr() as *const f32, elements)
        };
        
        // Verify results
        for i in 0..elements {
            let expected = data_a[i] + data_b[i];
            assert!((result_floats[i] - expected).abs() < 0.001,
                    "Mismatch at {}: {} != {}", i, result_floats[i], expected);
        }
        
        println!("Kernel execution test passed!");
    }
}

#[tokio::test]
async fn test_gpu_monitoring() {
    if !is_cuda_available() {
        println!("CUDA not available, skipping test");
        return;
    }
    
    let manager = GpuManager::new().expect("Failed to create GPU manager");
    
    let metrics = manager.get_metrics();
    if metrics.is_empty() {
        println!("GPU monitoring not available (NVML not installed?)");
        return;
    }
    
    for m in metrics {
        println!("GPU {} ({}): {}°C, {}W, {}% utilization",
                 m.index, m.name, m.temperature, m.power_usage, m.gpu_utilization);
        assert!(m.temperature > 0 && m.temperature < 120); // Reasonable temp range
    }
}

#[tokio::test]
async fn test_benchmark() {
    if !is_cuda_available() {
        println!("CUDA not available, skipping test");
        return;
    }
    
    let manager = GpuManager::new().expect("Failed to create GPU manager");
    
    println!("Running GPU benchmark...");
    let results = manager.benchmark_all().await;
    
    assert!(!results.is_empty());
    
    for (idx, result) in results {
        println!("GPU {}: {:.2} GB/s bandwidth, {:.2}ms kernel time",
                 idx, result.memory_bandwidth_gbps, result.kernel_time_ms);
        
        // Sanity checks
        assert!(result.memory_bandwidth_gbps > 0.0);
        assert!(result.kernel_time_ms > 0.0);
    }
}