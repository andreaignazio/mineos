use thiserror::Error;

#[derive(Error, Debug)]
pub enum GpuError {
    #[error("CUDA error: {0}")]
    Cuda(#[from] cudarc::driver::DriverError),
    
    #[error("NVML error: {0}")]
    Nvml(#[from] nvml_wrapper::error::NvmlError),
    
    #[error("No CUDA devices found")]
    NoDevicesFound,
    
    #[error("Device index {0} out of range")]
    InvalidDeviceIndex(usize),
    
    #[error("Failed to allocate {0} bytes on GPU")]
    AllocationFailed(usize),
    
    #[error("Kernel compilation failed: {0}")]
    KernelCompilationFailed(String),
    
    #[error("Kernel launch failed: {0}")]
    KernelLaunchFailed(String),
    
    #[error("Memory transfer failed: {0}")]
    MemoryTransferFailed(String),
    
    #[error("GPU {0} is not available")]
    DeviceUnavailable(usize),
    
    #[error("Feature not supported: {0}")]
    NotSupported(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, GpuError>;