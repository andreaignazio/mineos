use nvml_wrapper::{Nvml, Device as NvmlDevice};
use nvml_wrapper::struct_wrappers::device::Utilization;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

use crate::cuda::error::{GpuError, Result};

/// GPU monitoring information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GpuMetrics {
    pub index: u32,
    pub name: String,
    pub temperature: u32,  // Celsius
    pub power_usage: u32,  // Watts
    pub power_limit: u32,  // Watts
    pub fan_speed: u32,    // Percentage
    pub memory_used: u64,  // Bytes
    pub memory_total: u64, // Bytes
    pub gpu_utilization: u32,  // Percentage
    pub memory_utilization: u32, // Percentage
    pub pcie_throughput_rx: u32, // MB/s
    pub pcie_throughput_tx: u32, // MB/s
}

/// NVML-based GPU monitor
pub struct GpuMonitor {
    nvml: Arc<Nvml>,
    devices: Vec<NvmlDevice<'static>>,
}

impl GpuMonitor {
    /// Create a new GPU monitor
    pub fn new() -> Result<Self> {
        info!("Initializing NVML for GPU monitoring");
        
        let nvml = Nvml::init().map_err(|e| {
            warn!("Failed to initialize NVML: {}", e);
            e
        })?;
        
        let device_count = nvml.device_count()?;
        info!("NVML found {} GPU(s)", device_count);
        
        let mut devices = Vec::new();
        for i in 0..device_count {
            match nvml.device_by_index(i) {
                Ok(device) => {
                    // We need to leak the device to get 'static lifetime
                    // This is safe as we keep the Nvml instance alive
                    let device: NvmlDevice<'static> = unsafe {
                        std::mem::transmute(device)
                    };
                    devices.push(device);
                }
                Err(e) => {
                    warn!("Failed to get device {}: {}", i, e);
                }
            }
        }
        
        Ok(Self {
            nvml: Arc::new(nvml),
            devices,
        })
    }
    
    /// Get metrics for a specific GPU
    pub fn get_metrics(&self, index: usize) -> Result<GpuMetrics> {
        if index >= self.devices.len() {
            return Err(GpuError::InvalidDeviceIndex(index));
        }
        
        let device = &self.devices[index];
        
        // Get basic info
        let name = device.name()?;
        
        // Temperature
        let temperature = device.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
            .unwrap_or(0);
        
        // Power
        let power_usage = device.power_usage()
            .map(|p| p / 1000)  // Convert mW to W
            .unwrap_or(0);
        
        let power_limit = device.power_management_limit()
            .map(|p| p / 1000)  // Convert mW to W
            .unwrap_or(0);
        
        // Fan speed
        let fan_speed = device.fan_speed(0)
            .unwrap_or(0);
        
        // Memory
        let mem_info = device.memory_info()?;
        let memory_used = mem_info.used;
        let memory_total = mem_info.total;
        
        // Utilization
        let utilization = device.utilization_rates()
            .unwrap_or(Utilization {
                gpu: 0,
                memory: 0,
            });
        
        // PCIe throughput (if available)
        // PCIe throughput not available in v0.10
        let pcie_throughput_rx = 0;
        let pcie_throughput_tx = 0;
        
        Ok(GpuMetrics {
            index: index as u32,
            name,
            temperature,
            power_usage,
            power_limit,
            fan_speed,
            memory_used,
            memory_total,
            gpu_utilization: utilization.gpu,
            memory_utilization: utilization.memory,
            pcie_throughput_rx,
            pcie_throughput_tx,
        })
    }
    
    /// Get metrics for all GPUs
    pub fn get_all_metrics(&self) -> Vec<GpuMetrics> {
        let mut metrics = Vec::new();
        
        for i in 0..self.devices.len() {
            match self.get_metrics(i) {
                Ok(m) => metrics.push(m),
                Err(e) => {
                    warn!("Failed to get metrics for GPU {}: {}", i, e);
                }
            }
        }
        
        metrics
    }
    
    /// Set power limit for a GPU (requires admin privileges)
    pub fn set_power_limit(&mut self, index: usize, watts: u32) -> Result<()> {
        if index >= self.devices.len() {
            return Err(GpuError::InvalidDeviceIndex(index));
        }
        
        let device = &mut self.devices[index];
        let milliwatts = watts * 1000;
        
        device.set_power_management_limit(milliwatts)?;
        info!("Set power limit for GPU {} to {}W", index, watts);
        
        Ok(())
    }
    
    /// Set GPU clock speeds (requires admin privileges)
    pub fn set_gpu_clocks(&mut self, index: usize, min_mhz: u32, max_mhz: u32) -> Result<()> {
        if index >= self.devices.len() {
            return Err(GpuError::InvalidDeviceIndex(index));
        }
        
        let device = &mut self.devices[index];
        
        // Use GpuLockedClocksSetting enum
        use nvml_wrapper::enums::device::GpuLockedClocksSetting;
        let setting = GpuLockedClocksSetting::Numeric {
            min_clock_mhz: min_mhz,
            max_clock_mhz: max_mhz,
        };
        device.set_gpu_locked_clocks(setting)?;
        info!("Set GPU {} clocks to {}-{} MHz", index, min_mhz, max_mhz);
        
        Ok(())
    }
    
    /// Reset GPU clocks to default
    pub fn reset_gpu_clocks(&mut self, index: usize) -> Result<()> {
        if index >= self.devices.len() {
            return Err(GpuError::InvalidDeviceIndex(index));
        }
        
        let device = &mut self.devices[index];
        
        device.reset_gpu_locked_clocks()?;
        info!("Reset GPU {} clocks to default", index);
        
        Ok(())
    }
    
    /// Check if GPU needs thermal throttling
    pub fn check_thermal_throttle(&self, index: usize, max_temp: u32) -> Result<bool> {
        let metrics = self.get_metrics(index)?;
        Ok(metrics.temperature >= max_temp)
    }
}

/// Continuous monitoring task
pub struct MonitoringTask {
    monitor: Arc<Mutex<GpuMonitor>>,
    interval_ms: u64,
    running: Arc<Mutex<bool>>,
}

impl MonitoringTask {
    /// Create a new monitoring task
    pub fn new(monitor: GpuMonitor, interval_ms: u64) -> Self {
        Self {
            monitor: Arc::new(Mutex::new(monitor)),
            interval_ms,
            running: Arc::new(Mutex::new(false)),
        }
    }
    
    /// Start monitoring in background
    pub async fn start<F>(&self, callback: F) 
    where
        F: Fn(Vec<GpuMetrics>) + Send + 'static,
    {
        *self.running.lock().unwrap() = true;
        let monitor = self.monitor.clone();
        let running = self.running.clone();
        let interval_ms = self.interval_ms;
        
        tokio::spawn(async move {
            while *running.lock().unwrap() {
                // Get metrics
                let metrics = {
                    let mon = monitor.lock().unwrap();
                    mon.get_all_metrics()
                };
                
                // Call callback
                callback(metrics);
                
                // Sleep
                tokio::time::sleep(tokio::time::Duration::from_millis(interval_ms)).await;
            }
        });
    }
    
    /// Stop monitoring
    pub fn stop(&self) {
        *self.running.lock().unwrap() = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_nvml_init() {
        // This test will only pass on systems with NVIDIA GPUs
        match GpuMonitor::new() {
            Ok(monitor) => {
                let metrics = monitor.get_all_metrics();
                for m in metrics {
                    println!("GPU {}: {} - {}°C, {}W", 
                             m.index, m.name, m.temperature, m.power_usage);
                }
            }
            Err(e) => {
                println!("NVML not available: {}", e);
            }
        }
    }
}