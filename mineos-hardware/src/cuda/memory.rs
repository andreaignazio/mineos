use cudarc::driver::{CudaStream, CudaSlice};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use tracing::{debug, info, warn};

use super::error::{GpuError, Result};

/// Memory pool for efficient GPU memory management
pub struct MemoryPool {
    stream: Arc<CudaStream>,
    pools: RwLock<HashMap<usize, Vec<MemoryBlock>>>,
    allocated_bytes: Arc<Mutex<usize>>,
    max_pool_size: usize,
}

struct MemoryBlock {
    slice: CudaSlice<u8>,
    size: usize,
    in_use: bool,
}

impl MemoryPool {
    /// Create a new memory pool
    pub fn new(stream: Arc<CudaStream>, max_pool_size: usize) -> Self {
        Self {
            stream,
            pools: RwLock::new(HashMap::new()),
            allocated_bytes: Arc::new(Mutex::new(0)),
            max_pool_size,
        }
    }
    
    /// Allocate memory from the pool
    pub fn allocate(&self, size: usize) -> Result<PooledBuffer> {
        // Round up to nearest power of 2 for better pooling
        let pool_size = size.next_power_of_two();
        
        // Try to get from pool first
        {
            let mut pools = self.pools.write().unwrap();
            if let Some(blocks) = pools.get_mut(&pool_size) {
                for block in blocks.iter_mut() {
                    if !block.in_use {
                        block.in_use = true;
                        debug!("Reusing pooled buffer of size {}", pool_size);
                        
                        return Ok(PooledBuffer {
                            slice: block.slice.clone(),
                            size,
                            pool_size,
                            pool: self as *const _ as *mut MemoryPool,
                        });
                    }
                }
            }
        }
        
        // Check if we can allocate more
        let current_allocated = *self.allocated_bytes.lock().unwrap();
        if current_allocated + pool_size > self.max_pool_size {
            warn!("Memory pool limit reached: {} + {} > {}", 
                  current_allocated, pool_size, self.max_pool_size);
            return Err(GpuError::AllocationFailed(pool_size));
        }
        
        // Allocate new block using stream
        info!("Allocating new GPU buffer of size {}", pool_size);
        let slice = unsafe {
            self.stream.alloc::<u8>(pool_size)?
        };
        
        // Add to pool
        {
            let mut pools = self.pools.write().unwrap();
            let blocks = pools.entry(pool_size).or_insert_with(Vec::new);
            blocks.push(MemoryBlock {
                slice: slice.clone(),
                size: pool_size,
                in_use: true,
            });
        }
        
        // Update allocated bytes
        *self.allocated_bytes.lock().unwrap() += pool_size;
        
        Ok(PooledBuffer {
            slice,
            size,
            pool_size,
            pool: self as *const _ as *mut MemoryPool,
        })
    }
    
    /// Return a buffer to the pool
    fn return_buffer(&self, slice: &CudaSlice<u8>, pool_size: usize) {
        let mut pools = self.pools.write().unwrap();
        if let Some(blocks) = pools.get_mut(&pool_size) {
            for block in blocks.iter_mut() {
                // Compare device slices - check if they point to same memory
                // Since we can't access cu_device_ptr directly, compare by reference
                if std::ptr::eq(&block.slice as *const _, slice as *const _) {
                    block.in_use = false;
                    debug!("Returned buffer of size {} to pool", pool_size);
                    return;
                }
            }
        }
        warn!("Buffer not found in pool, possible memory leak");
    }
    
    /// Clear all unused buffers from the pool
    pub fn clear_unused(&self) -> usize {
        let mut freed = 0;
        let mut pools = self.pools.write().unwrap();
        
        for (size, blocks) in pools.iter_mut() {
            blocks.retain(|block| {
                if !block.in_use {
                    freed += size;
                    false
                } else {
                    true
                }
            });
        }
        
        // Remove empty entries
        pools.retain(|_, blocks| !blocks.is_empty());
        
        // Update allocated bytes
        *self.allocated_bytes.lock().unwrap() -= freed;
        
        info!("Cleared {} bytes from memory pool", freed);
        freed
    }
    
    /// Get total allocated memory
    pub fn allocated_bytes(&self) -> usize {
        *self.allocated_bytes.lock().unwrap()
    }
    
    /// Get pool statistics
    pub fn stats(&self) -> MemoryPoolStats {
        let pools = self.pools.read().unwrap();
        let mut total_blocks = 0;
        let mut used_blocks = 0;
        let mut pool_sizes = Vec::new();
        
        for (size, blocks) in pools.iter() {
            total_blocks += blocks.len();
            used_blocks += blocks.iter().filter(|b| b.in_use).count();
            pool_sizes.push(*size);
        }
        
        MemoryPoolStats {
            total_blocks,
            used_blocks,
            free_blocks: total_blocks - used_blocks,
            allocated_bytes: *self.allocated_bytes.lock().unwrap(),
            max_pool_size: self.max_pool_size,
            pool_sizes,
        }
    }
}

/// A buffer allocated from the memory pool
pub struct PooledBuffer {
    slice: CudaSlice<u8>,
    size: usize,
    pool_size: usize,
    pool: *mut MemoryPool,
}

impl PooledBuffer {
    /// Get the size of the buffer
    pub fn size(&self) -> usize {
        self.size
    }
    
    /// Get the device slice
    pub fn as_device_slice(&self) -> &CudaSlice<u8> {
        &self.slice
    }
    
    /// Get mutable device slice
    pub fn as_device_slice_mut(&mut self) -> &mut CudaSlice<u8> {
        &mut self.slice
    }
    
    /// Copy data from host to device
    pub fn copy_from_host(&mut self, data: &[u8]) -> Result<()> {
        if data.len() > self.size {
            return Err(GpuError::MemoryTransferFailed(
                format!("Data size {} exceeds buffer size {}", data.len(), self.size)
            ));
        }
        
        // Get stream from pool
        let pool = unsafe { &*self.pool };
        pool.stream.memcpy_htod(data, &mut self.slice)?;
        Ok(())
    }
    
    /// Copy data from device to host
    pub fn copy_to_host(&self) -> Result<Vec<u8>> {
        // Get stream from pool
        let pool = unsafe { &*self.pool };
        Ok(pool.stream.memcpy_dtov(&self.slice)?)
    }
    
    /// Copy to host into existing buffer
    pub fn copy_to_host_into(&self, data: &mut [u8]) -> Result<()> {
        if data.len() < self.size {
            return Err(GpuError::MemoryTransferFailed(
                format!("Target buffer size {} is smaller than device buffer size {}", 
                        data.len(), self.size)
            ));
        }
        
        // Get stream from pool
        let pool = unsafe { &*self.pool };
        let result = pool.stream.memcpy_dtov(&self.slice)?;
        data[..self.size].copy_from_slice(&result[..self.size]);
        Ok(())
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        // Return buffer to pool
        let pool = unsafe { &*self.pool };
        pool.return_buffer(&self.slice, self.pool_size);
    }
}

// Safety: PooledBuffer can be sent between threads
unsafe impl Send for PooledBuffer {}

/// Statistics about the memory pool
#[derive(Debug, Clone)]
pub struct MemoryPoolStats {
    pub total_blocks: usize,
    pub used_blocks: usize,
    pub free_blocks: usize,
    pub allocated_bytes: usize,
    pub max_pool_size: usize,
    pub pool_sizes: Vec<usize>,
}

/// Simple pinned memory wrapper for fast transfers
pub struct PinnedMemory {
    stream: Arc<CudaStream>,
}

impl PinnedMemory {
    /// Create a new pinned memory manager
    pub fn new(stream: Arc<CudaStream>) -> Self {
        Self { stream }
    }
    
    /// Allocate pinned host memory and copy to device
    pub fn allocate_and_copy(&self, data: &[u8]) -> Result<CudaSlice<u8>> {
        // Use memcpy_stod to create new slice with data
        Ok(self.stream.memcpy_stod(data)?)
    }
    
    /// Create a pinned buffer
    pub fn create_buffer(&self, size: usize) -> Result<PinnedBuffer> {
        // Allocate device memory
        let device_slice = unsafe {
            self.stream.alloc::<u8>(size)?
        };
        
        Ok(PinnedBuffer {
            size,
            device_slice,
            stream: self.stream.clone(),
        })
    }
}

/// Pinned memory buffer for fast transfers
pub struct PinnedBuffer {
    size: usize,
    device_slice: CudaSlice<u8>,
    stream: Arc<CudaStream>,
}

impl PinnedBuffer {
    /// Copy data to device
    pub fn copy_to_device(&mut self, data: &[u8]) -> Result<()> {
        if data.len() != self.size {
            return Err(GpuError::MemoryTransferFailed(
                format!("Data size {} doesn't match buffer size {}", data.len(), self.size)
            ));
        }
        
        self.stream.memcpy_htod(data, &mut self.device_slice)?;
        Ok(())
    }
    
    /// Copy data from device
    pub fn copy_from_device(&self) -> Result<Vec<u8>> {
        Ok(self.stream.memcpy_dtov(&self.device_slice)?)
    }
    
    /// Get device slice
    pub fn device_slice(&self) -> &CudaSlice<u8> {
        &self.device_slice
    }
}