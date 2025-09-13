use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use once_cell::sync::OnceCell;

static CONFIG_PATH: OnceCell<PathBuf> = OnceCell::new();

/// Main miner configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinerConfig {
    pub algorithm: String,
    pub worker_name: String,
    pub pool: PoolConfig,
    pub gpus: GpuConfig,
    pub monitoring: MonitoringConfig,
    pub profit_switching: Option<ProfitSwitchingConfig>,
}

/// Pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    pub url: String,
    pub wallet: String,
    pub password: String,
}

/// GPU configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuConfig {
    pub enabled: Vec<usize>,
    pub overclocks: Vec<GpuOverclock>,
}

/// GPU overclock settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuOverclock {
    pub index: usize,
    pub core_clock: i32,
    pub memory_clock: i32,
    pub power_limit: u32,
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub update_interval: u32,
    pub temperature_limit: u32,
}

/// Profit switching configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfitSwitchingConfig {
    pub enabled: bool,
    pub check_interval: u32,
    pub threshold: f64,
    pub algorithms: Vec<String>,
}

/// Get the config file path
pub fn get_config_path() -> PathBuf {
    CONFIG_PATH.get().cloned().unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mineos")
            .join("config.toml")
    })
}

/// Set custom config path
pub fn set_config_path(path: &str) {
    let _ = CONFIG_PATH.set(PathBuf::from(path));
}

/// Check if config exists
pub fn config_exists() -> bool {
    get_config_path().exists()
}

/// Load configuration from file
pub fn load_config() -> Result<MinerConfig> {
    let config_path = get_config_path();

    if !config_path.exists() {
        return Err(anyhow::anyhow!(
            "Configuration file not found at {}. Run 'mineos setup' to create one.",
            config_path.display()
        ));
    }

    let contents = fs::read_to_string(&config_path)?;
    let config: MinerConfig = toml::from_str(&contents)?;

    Ok(config)
}

/// Save configuration to file
pub fn save_config(config: &MinerConfig) -> Result<()> {
    let config_path = get_config_path();

    // Create directory if it doesn't exist
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = toml::to_string_pretty(config)?;
    fs::write(&config_path, contents)?;

    Ok(())
}

/// Default configuration
impl Default for MinerConfig {
    fn default() -> Self {
        Self {
            algorithm: "kawpow".to_string(),
            worker_name: gethostname::gethostname()
                .to_string_lossy()
                .to_string(),
            pool: PoolConfig {
                url: "stratum+tcp://pool.example.com:3333".to_string(),
                wallet: "YOUR_WALLET_ADDRESS".to_string(),
                password: "x".to_string(),
            },
            gpus: GpuConfig {
                enabled: vec![0],
                overclocks: vec![],
            },
            monitoring: MonitoringConfig {
                update_interval: 1000,
                temperature_limit: 85,
            },
            profit_switching: None,
        }
    }
}