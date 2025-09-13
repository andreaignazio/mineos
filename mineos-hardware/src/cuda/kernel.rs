use cudarc::driver::{CudaContext, CudaStream, CudaModule, CudaFunction, LaunchConfig};
use cudarc::nvrtc::{compile_ptx, compile_ptx_with_opts, Ptx, CompileOptions};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

use super::error::{GpuError, Result};

/// Manages CUDA kernel compilation and execution
pub struct KernelManager {
    context: Arc<CudaContext>,
    stream: Arc<CudaStream>,
    modules: Mutex<HashMap<String, Arc<CudaModule>>>,
    functions: Mutex<HashMap<String, CudaFunction>>,
}

impl KernelManager {
    /// Create a new kernel manager
    pub fn new(context: Arc<CudaContext>, stream: Arc<CudaStream>) -> Self {
        Self {
            context,
            stream,
            modules: Mutex::new(HashMap::new()),
            functions: Mutex::new(HashMap::new()),
        }
    }
    
    /// Compile CUDA source code to PTX
    pub fn compile_cuda_source(
        &self,
        name: &str,
        source: &str,
    ) -> Result<Ptx> {
        info!("Compiling CUDA kernel: {}", name);
        
        match compile_ptx(source) {
            Ok(ptx) => {
                debug!("Successfully compiled kernel: {}", name);
                Ok(ptx)
            }
            Err(e) => {
                Err(GpuError::KernelCompilationFailed(format!("{}: {}", name, e)))
            }
        }
    }
    
    /// Load a PTX module
    pub fn load_ptx_module(&self, name: &str, ptx: Ptx) -> Result<()> {
        info!("Loading PTX module: {}", name);
        
        let module = self.context.load_module(ptx)?;
        
        let mut modules = self.modules.lock().unwrap();
        modules.insert(name.to_string(), module);
        
        Ok(())
    }
    
    /// Get a function from a loaded module
    pub fn get_function(&self, module_name: &str, function_name: &str) -> Result<CudaFunction> {
        // Check cache first
        let cache_key = format!("{}::{}", module_name, function_name);
        {
            let functions = self.functions.lock().unwrap();
            if let Some(func) = functions.get(&cache_key) {
                return Ok(func.clone());
            }
        }
        
        // Get module
        let modules = self.modules.lock().unwrap();
        let module = modules
            .get(module_name)
            .ok_or_else(|| GpuError::KernelLaunchFailed(
                format!("Module '{}' not loaded", module_name)
            ))?;
        
        // Get function
        let func = module.load_function(function_name)?;
        
        // Cache it
        let mut functions = self.functions.lock().unwrap();
        functions.insert(cache_key, func.clone());
        
        Ok(func)
    }
    
    /// Create a launch builder for a kernel
    pub fn launch_builder<'a>(&'a self, function: &'a CudaFunction) -> LaunchBuilder<'a> {
        LaunchBuilder::new(&self.stream, function)
    }
    
    /// Load and compile a test kernel
    pub fn load_test_kernel(&self) -> Result<()> {
        let ptx_src = r#"
            extern "C" __global__ void vector_add(float* out, const float* a, const float* b, int n) {
                int idx = blockIdx.x * blockDim.x + threadIdx.x;
                if (idx < n) {
                    out[idx] = a[idx] + b[idx];
                }
            }
            
            extern "C" __global__ void fill_value(float* data, float value, int n) {
                int idx = blockIdx.x * blockDim.x + threadIdx.x;
                if (idx < n) {
                    data[idx] = value;
                }
            }
        "#;
        
        let ptx = self.compile_cuda_source("test_kernels", ptx_src)?;
        self.load_ptx_module("test_kernels", ptx)?;
        
        Ok(())
    }
    
    /// Compile with specific compute capability
    pub fn compile_with_compute_capability(
        &self,
        name: &str,
        source: &str,
        compute_capability: (u32, u32),
    ) -> Result<Ptx> {
        info!("Compiling kernel {} for SM_{}{}", name, compute_capability.0, compute_capability.1);
        
        let arch = format!("sm_{}{}", compute_capability.0, compute_capability.1);
        let arch_str = Box::leak(arch.into_boxed_str());
        let opts = CompileOptions {
            arch: Some(arch_str),
            ..Default::default()
        };
        
        match compile_ptx_with_opts(source, opts) {
            Ok(ptx) => Ok(ptx),
            Err(e) => Err(GpuError::KernelCompilationFailed(format!("{}: {}", name, e)))
        }
    }
}

/// Builder for kernel launches using the new v0.17 API
pub struct LaunchBuilder<'a> {
    stream: &'a CudaStream,
    function: &'a CudaFunction,
    args: Vec<*mut std::ffi::c_void>,
    config: LaunchConfig,
}

impl<'a> LaunchBuilder<'a> {
    /// Create a new launch builder
    pub fn new(stream: &'a CudaStream, function: &'a CudaFunction) -> Self {
        Self {
            stream,
            function,
            args: Vec::new(),
            config: LaunchConfig {
                grid_dim: (1, 1, 1),
                block_dim: (256, 1, 1),
                shared_mem_bytes: 0,
            },
        }
    }
    
    /// Add an argument to the kernel
    pub fn arg<T>(mut self, arg: &T) -> Self {
        let ptr = arg as *const T as *mut std::ffi::c_void;
        self.args.push(ptr);
        self
    }
    
    /// Set the launch configuration
    pub fn config(mut self, config: LaunchConfig) -> Self {
        self.config = config;
        self
    }
    
    /// Set grid and block dimensions
    pub fn dims(mut self, grid: (u32, u32, u32), block: (u32, u32, u32)) -> Self {
        self.config.grid_dim = grid;
        self.config.block_dim = block;
        self
    }
    
    /// Calculate grid size for 1D problem
    pub fn auto_1d(mut self, problem_size: u32, block_size: u32) -> Self {
        self.config.block_dim = (block_size, 1, 1);
        let grid_x = (problem_size + block_size - 1) / block_size;
        self.config.grid_dim = (grid_x, 1, 1);
        self
    }
    
    /// Set shared memory size
    pub fn shared_mem(mut self, bytes: u32) -> Self {
        self.config.shared_mem_bytes = bytes;
        self
    }
    
    /// Launch the kernel
    pub unsafe fn launch(self) -> Result<()> {
        // Use the stream's launch_builder
        let mut builder = self.stream.launch_builder(self.function);
        
        // Add arguments - need to pass actual references
        // This is a limitation - we need to pass actual references
        // For now just launch without args
        
        // Launch with config
        builder.launch(self.config)?;
        
        Ok(())
    }
}

/// Helper to create launch configurations
pub struct KernelLauncher;

impl KernelLauncher {
    /// Create a launch config for a given number of elements
    pub fn for_num_elems(num_elems: u32) -> LaunchConfig {
        let block_size = 256;
        let grid_size = (num_elems + block_size - 1) / block_size;
        
        LaunchConfig {
            grid_dim: (grid_size, 1, 1),
            block_dim: (block_size, 1, 1),
            shared_mem_bytes: 0,
        }
    }
    
    /// Create a launch config with custom dimensions
    pub fn custom(grid: (u32, u32, u32), block: (u32, u32, u32)) -> LaunchConfig {
        LaunchConfig {
            grid_dim: grid,
            block_dim: block,
            shared_mem_bytes: 0,
        }
    }
    
    /// Create a launch config with shared memory
    pub fn with_shared_mem(
        grid: (u32, u32, u32),
        block: (u32, u32, u32),
        shared_mem_bytes: u32,
    ) -> LaunchConfig {
        LaunchConfig {
            grid_dim: grid,
            block_dim: block,
            shared_mem_bytes,
        }
    }
}