use anyhow::Result;
use dialoguer::{MultiSelect, theme::ColorfulTheme};
use nvml_wrapper::Nvml;

/// Represents a detected GPU
#[derive(Debug, Clone)]
pub struct DetectedGpu {
    pub index: usize,
    pub name: String,
    pub memory: u64,
    pub uuid: String,
}

/// Detect available GPUs on the system
pub fn detect_gpus() -> Result<Vec<DetectedGpu>> {
    let nvml = Nvml::init()?;
    let device_count = nvml.device_count()?;
    let mut gpus = Vec::new();

    for i in 0..device_count {
        let device = nvml.device_by_index(i)?;
        let name = device.name()?;
        let memory = device.memory_info()?.total;
        let uuid = device.uuid()?;

        gpus.push(DetectedGpu {
            index: i as usize,
            name,
            memory: memory / (1024 * 1024 * 1024), // Convert to GB
            uuid,
        });
    }

    Ok(gpus)
}

/// Interactive GPU selection
pub fn select_gpus(gpus: &[DetectedGpu]) -> Result<Vec<usize>> {
    if gpus.is_empty() {
        return Err(anyhow::anyhow!("No GPUs detected on the system"));
    }

    let gpu_names: Vec<String> = gpus
        .iter()
        .map(|gpu| format!("GPU {}: {} ({}GB)", gpu.index, gpu.name, gpu.memory))
        .collect();

    let defaults = vec![true; gpus.len()]; // Select all by default

    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select GPUs to use for mining")
        .items(&gpu_names)
        .defaults(&defaults)
        .interact()?;

    Ok(selections)
}

/// Get GPU information by index
pub fn get_gpu_info(index: usize) -> Result<DetectedGpu> {
    let nvml = Nvml::init()?;
    let device = nvml.device_by_index(index as u32)?;

    Ok(DetectedGpu {
        index,
        name: device.name()?,
        memory: device.memory_info()?.total / (1024 * 1024 * 1024),
        uuid: device.uuid()?,
    })
}

/// Check if GPU supports the given algorithm
pub fn supports_algorithm(gpu: &DetectedGpu, algorithm: &str) -> bool {
    // Minimum memory requirements per algorithm (in GB)
    let min_memory = match algorithm.to_lowercase().as_str() {
        "ethash" | "etchash" => 4,
        "kawpow" => 3,
        "autolykos2" => 2,
        "octopus" => 4,
        "firopow" => 3,
        _ => 2, // Default minimum
    };

    gpu.memory >= min_memory
}

/// Parse GPU list from command line (e.g., "0,1,2" or "all")
pub fn parse_gpu_list(gpu_string: &str) -> Result<Vec<usize>> {
    if gpu_string.to_lowercase() == "all" {
        let gpus = detect_gpus()?;
        Ok((0..gpus.len()).collect())
    } else {
        gpu_string
            .split(',')
            .map(|s| s.trim().parse::<usize>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to parse GPU list: {}", e))
    }
}

/// Get recommended settings for GPU and algorithm
pub fn get_recommended_settings(gpu: &DetectedGpu, algorithm: &str) -> (i32, i32, u32) {
    // Returns (core_clock, memory_clock, power_limit)
    // These are conservative starting points

    match gpu.name.as_str() {
        name if name.contains("3090") => match algorithm {
            "ethash" => (0, 1000, 300),
            "kawpow" => (100, 500, 320),
            _ => (0, 0, 300),
        },
        name if name.contains("3080") => match algorithm {
            "ethash" => (0, 900, 230),
            "kawpow" => (100, 400, 250),
            _ => (0, 0, 230),
        },
        name if name.contains("3070") => match algorithm {
            "ethash" => (0, 800, 130),
            "kawpow" => (100, 300, 150),
            _ => (0, 0, 130),
        },
        name if name.contains("3060") => match algorithm {
            "ethash" => (0, 700, 120),
            "kawpow" => (50, 200, 130),
            _ => (0, 0, 120),
        },
        _ => (0, 0, 100), // Conservative defaults
    }
}