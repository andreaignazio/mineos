use cudarc::driver::{CudaContext, CudaStream, CudaSlice, sys};
use std::sync::Arc;
use tracing::{debug, info};

use super::error::{GpuError, Result};

/// Represents a single CUDA-capable GPU device
#[derive(Clone)]
pub struct GpuDevice {
    /// Device index (0-based)
    pub index: usize,
    
    /// Device name
    pub name: String,
    
    /// Total memory in bytes
    pub total_memory: usize,
    
    /// Compute capability (major, minor)
    pub compute_capability: (u32, u32),
    
    /// Number of multiprocessors
    pub multiprocessor_count: u32,
    
    /// Clock rate in MHz
    pub clock_rate_mhz: u32,
    
    /// CUDA context for this device
    context: Arc<CudaContext>,
    
    /// Default stream for this device
    stream: Arc<CudaStream>,
}

impl GpuDevice {
    /// Create a new GPU device wrapper
    pub fn new(index: usize) -> Result<Self> {
        info!("Initializing GPU device {}", index);
        
        // Create CUDA context for this device - returns Arc<CudaContext>
        let context = CudaContext::new(index)?;
        let stream = context.default_stream();
        
        // Get device name
        let name = context.name()?;
        
        // Get compute capability using attributes
        let major = context.attribute(sys::CUdevice_attribute::CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR)? as u32;
        let minor = context.attribute(sys::CUdevice_attribute::CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MINOR)? as u32;
        
        // Get multiprocessor count
        let multiprocessor_count = context.attribute(sys::CUdevice_attribute::CU_DEVICE_ATTRIBUTE_MULTIPROCESSOR_COUNT)? as u32;
        
        // Get clock rate (in KHz from CUDA)
        let clock_rate_khz = context.attribute(sys::CUdevice_attribute::CU_DEVICE_ATTRIBUTE_CLOCK_RATE)? as u32;
        let clock_rate_mhz = clock_rate_khz / 1000;
        
        // Get total memory - we'll need to use CUDA sys API
        let total_memory = get_total_memory(&context)?;
        
        info!("GPU {}: {} with {}MB memory, compute capability {}.{}", 
              index, name, total_memory / 1024 / 1024, major, minor);
        
        Ok(Self {
            index,
            name,
            total_memory,
            compute_capability: (major, minor),
            multiprocessor_count,
            clock_rate_mhz,
            context: context,
            stream: stream,
        })
    }
    
    /// Get the CUDA context
    pub fn context(&self) -> &Arc<CudaContext> {
        &self.context
    }
    
    /// Get the default stream
    pub fn stream(&self) -> &Arc<CudaStream> {
        &self.stream
    }
    
    /// Create a new stream for async operations
    pub fn create_stream(&self) -> Result<Arc<CudaStream>> {
        Ok(self.context.new_stream()?)
    }
    
    /// Allocate memory on the GPU
    pub fn allocate<T: cudarc::driver::DeviceRepr>(&self, len: usize) -> Result<DeviceBuffer<T>> {
        debug!("Allocating {} elements on GPU {}", len, self.index);
        
        // Allocate uninitialized memory
        let slice = unsafe {
            self.stream.alloc::<T>(len)?
        };
        
        Ok(DeviceBuffer {
            slice,
            stream: self.stream.clone(),
        })
    }
    
    /// Allocate and zero memory on the GPU
    pub fn allocate_zeros<T>(&self, len: usize) -> Result<DeviceBuffer<T>> 
    where
        T: cudarc::driver::DeviceRepr + cudarc::driver::ValidAsZeroBits,
    {
        debug!("Allocating {} zeroed elements on GPU {}", len, self.index);
        
        let slice = self.stream.alloc_zeros::<T>(len)?;
        
        Ok(DeviceBuffer {
            slice,
            stream: self.stream.clone(),
        })
    }
    
    /// Get available memory in bytes
    pub fn available_memory(&self) -> Result<usize> {
        let (free, _total) = get_memory_info(&self.context)?;
        Ok(free)
    }
    
    /// Get memory usage percentage
    pub fn memory_usage_percent(&self) -> Result<f32> {
        let (free, total) = get_memory_info(&self.context)?;
        let used = total - free;
        Ok((used as f32 / total as f32) * 100.0)
    }
    
    /// Synchronize the device (wait for all operations to complete)
    pub fn synchronize(&self) -> Result<()> {
        self.stream.synchronize()?;
        Ok(())
    }
}

/// Get total memory for a device
fn get_total_memory(context: &CudaContext) -> Result<usize> {
    // Use the CU_DEVICE_ATTRIBUTE_TOTAL_CONSTANT_MEMORY attribute as a proxy
    // or we can get it from memory info
    let (_, total) = get_memory_info(context)?;
    Ok(total)
}

/// Get memory info using CUDA sys API
fn get_memory_info(context: &CudaContext) -> Result<(usize, usize)> {
    unsafe {
        let mut free = 0;
        let mut total = 0;
        
        // Make this context current
        context.bind_to_thread()?;
        
        // Get memory info
        let result = sys::cuMemGetInfo_v2(&mut free, &mut total);
        
        if result == sys::CUresult::CUDA_SUCCESS {
            Ok((free, total))
        } else {
            Err(GpuError::Cuda(cudarc::driver::DriverError(result)))
        }
    }
}

/// A buffer allocated on the GPU
pub struct DeviceBuffer<T> {
    slice: CudaSlice<T>,
    stream: Arc<CudaStream>,
}

impl<T> DeviceBuffer<T> {
    /// Get the length of the buffer in elements
    pub fn len(&self) -> usize {
        self.slice.len()
    }
    
    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.slice.len() == 0
    }
    
    /// Get the CudaSlice
    pub fn slice(&self) -> &CudaSlice<T> {
        &self.slice
    }
    
    /// Get mutable CudaSlice
    pub fn slice_mut(&mut self) -> &mut CudaSlice<T> {
        &mut self.slice
    }
}

impl<T: Clone> DeviceBuffer<T> {
    /// Copy data from host to device
    pub fn copy_from_host(&mut self, data: &[T]) -> Result<()> 
    where
        T: cudarc::driver::DeviceRepr,
    {
        if data.len() != self.len() {
            return Err(GpuError::MemoryTransferFailed(
                format!("Size mismatch: buffer has {} elements, data has {} elements", 
                        self.len(), data.len())
            ));
        }
        
        // Copy data to device using stream
        self.stream.memcpy_htod(data, &mut self.slice)?;
        Ok(())
    }
    
    /// Copy data from device to host
    pub fn copy_to_host(&self) -> Result<Vec<T>> 
    where
        T: cudarc::driver::DeviceRepr + Clone,
    {
        Ok(self.stream.memcpy_dtov(&self.slice)?)
    }
}

impl<T> Drop for DeviceBuffer<T> {
    fn drop(&mut self) {
        debug!("Freeing {} elements on GPU", self.len());
        // cudarc handles deallocation automatically when CudaSlice is dropped
    }
}