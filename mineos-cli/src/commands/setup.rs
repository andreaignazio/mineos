use anyhow::Result;
use clap::Args;
use colored::*;
use console::style;
use dialoguer::{Input, Select, Confirm};
use indicatif::{ProgressBar, ProgressStyle};

use crate::config::{MinerConfig, PoolConfig, GpuConfig, save_config};
use crate::client::MinerClient;

/// Interactive setup wizard
#[derive(Args)]
pub struct SetupArgs {
    /// Skip hardware detection
    #[arg(long)]
    skip_detection: bool,

    /// Reconfigure existing setup
    #[arg(long)]
    reconfigure: bool,
}

pub async fn execute(args: SetupArgs) -> Result<()> {
    use crate::utils::banner;

    // Show fancy welcome
    banner::show_box("⚙️  MineOS Setup Wizard", vec![
        "This wizard will help you configure your mining setup.",
        "We'll detect your hardware and set up your mining pools.",
        "",
        "Let's get started!"
    ]);
    println!();

    // Check for existing config
    if !args.reconfigure && crate::config::config_exists() {
        let overwrite = Confirm::new()
            .with_prompt("Configuration already exists. Overwrite?")
            .default(false)
            .interact()?;

        if !overwrite {
            println!("{}", "Setup cancelled".yellow());
            return Ok(());
        }
    }

    // Hardware detection
    let gpu_count = if !args.skip_detection {
        detect_hardware().await?
    } else {
        1
    };

    // Algorithm selection
    let algorithm = select_algorithm()?;

    // Pool configuration
    let pool = configure_pool(&algorithm)?;

    // Worker name
    let worker_name = Input::<String>::new()
        .with_prompt("Worker name")
        .default(get_default_worker_name())
        .interact()?;

    // GPU selection
    let gpus = select_gpus(gpu_count)?;

    // Monitoring settings
    let (temp_limit, update_interval) = configure_monitoring()?;

    // Profit switching
    let profit_switching = configure_profit_switching()?;

    // Build configuration
    let config = MinerConfig {
        algorithm,
        worker_name,
        pool,
        gpus: GpuConfig {
            enabled: gpus,
            overclocks: vec![],
        },
        monitoring: crate::config::MonitoringConfig {
            update_interval,
            temperature_limit: temp_limit,
        },
        profit_switching,
    };

    // Save configuration
    save_config(&config)?;

    // Show summary
    show_summary(&config);

    // Offer to start mining
    let start_now = Confirm::new()
        .with_prompt("Start mining now?")
        .default(true)
        .interact()?;

    if start_now {
        println!();
        banner::show_box("🎉 Setup Complete!", vec![
            "Your mining configuration has been saved.",
            "",
            "Starting miner now..."
        ]);
        println!();
        crate::commands::start::execute(crate::commands::start::StartArgs {
            algorithm: None,
            pool: None,
            wallet: None,
            worker_name: None,
            gpus: "all".to_string(),
            benchmark: false,
            dry_run: false,
        }).await?;
    }

    Ok(())
}

async fn detect_hardware() -> Result<usize> {
    println!("{}", style("Detecting hardware...").dim());

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")?
    );
    pb.set_message("Detecting GPUs...");

    // Detect GPUs
    let gpus = crate::utils::gpu_selector::detect_gpus()?;

    pb.finish_and_clear();

    println!("{}", format!("✓ Found {} GPU(s):", gpus.len()).green());
    for gpu in &gpus {
        println!("  GPU {}: {} ({} GB)",
            gpu.index,
            gpu.name.yellow(),
            gpu.memory
        );
    }
    println!();

    Ok(gpus.len())
}

fn select_algorithm() -> Result<String> {
    let algorithms = vec![
        ("KawPow", "kawpow", "Ravencoin (RVN)"),
        ("Ethash", "ethash", "Ethereum Classic (ETC)"),
        ("KHeavyHash", "kheavyhash", "Kaspa (KAS)"),
        ("Autolykos2", "autolykos2", "Ergo (ERG)"),
    ];

    let display_items: Vec<String> = algorithms
        .iter()
        .map(|(name, _, coin)| format!("{} - {}", name, coin))
        .collect();

    let selection = Select::new()
        .with_prompt("Select mining algorithm")
        .items(&display_items)
        .default(0)
        .interact()?;

    Ok(algorithms[selection].1.to_string())
}

fn configure_pool(algorithm: &str) -> Result<PoolConfig> {
    println!("\n{}", "Pool Configuration".bold());

    // Suggest pools based on algorithm
    let suggested_pools = get_suggested_pools(algorithm);

    let pool_choice = Select::new()
        .with_prompt("Select pool")
        .items(&{
            let mut items = vec!["Enter custom pool URL"];
            items.extend(suggested_pools.iter().map(|s| &**s));
            items
        })
        .interact()?;

    let pool_url = if pool_choice == 0 {
        Input::<String>::new()
            .with_prompt("Pool URL (e.g., stratum+tcp://pool.example.com:3333)")
            .validate_with(|input: &String| -> Result<(), &str> {
                if input.starts_with("stratum+tcp://") || input.starts_with("stratum+ssl://") {
                    Ok(())
                } else {
                    Err("Pool URL must start with stratum+tcp:// or stratum+ssl://")
                }
            })
            .interact()?
    } else {
        suggested_pools[pool_choice - 1].split(" - ").next().unwrap().to_string()
    };

    let wallet = Input::<String>::new()
        .with_prompt(format!("Wallet address for {}", algorithm))
        .validate_with(|input: &String| -> Result<(), &str> {
            if input.len() > 20 {
                Ok(())
            } else {
                Err("Invalid wallet address")
            }
        })
        .interact()?;

    let password = Input::<String>::new()
        .with_prompt("Pool password (usually 'x')")
        .default("x".to_string())
        .interact()?;

    Ok(PoolConfig {
        url: pool_url,
        wallet,
        password,
    })
}

fn select_gpus(gpu_count: usize) -> Result<Vec<usize>> {
    if gpu_count == 1 {
        return Ok(vec![0]);
    }

    let use_all = Confirm::new()
        .with_prompt("Use all GPUs for mining?")
        .default(true)
        .interact()?;

    if use_all {
        Ok((0..gpu_count).collect())
    } else {
        println!("Select GPUs to use (space to select, enter to confirm):");
        let gpu_options: Vec<String> = (0..gpu_count)
            .map(|i| format!("GPU {}", i))
            .collect();

        let selections = dialoguer::MultiSelect::new()
            .items(&gpu_options)
            .interact()?;

        Ok(selections)
    }
}

fn configure_monitoring() -> Result<(u32, u32)> {
    println!("\n{}", "Monitoring Settings".bold());

    let temp_limit = Input::<u32>::new()
        .with_prompt("Temperature limit (°C)")
        .default(85)
        .validate_with(|input: &u32| -> Result<(), &str> {
            if *input >= 60 && *input <= 95 {
                Ok(())
            } else {
                Err("Temperature must be between 60-95°C")
            }
        })
        .interact()?;

    let update_interval = Input::<u32>::new()
        .with_prompt("Update interval (ms)")
        .default(1000)
        .interact()?;

    Ok((temp_limit, update_interval))
}

fn configure_profit_switching() -> Result<Option<crate::config::ProfitSwitchingConfig>> {
    let enable = Confirm::new()
        .with_prompt("Enable automatic profit switching?")
        .default(false)
        .interact()?;

    if !enable {
        return Ok(None);
    }

    let check_interval = Input::<u32>::new()
        .with_prompt("Check interval (seconds)")
        .default(300)
        .interact()?;

    let threshold = Input::<f64>::new()
        .with_prompt("Switch threshold (%)")
        .default(5.0)
        .interact()?;

    Ok(Some(crate::config::ProfitSwitchingConfig {
        enabled: true,
        check_interval,
        threshold,
        algorithms: vec![],
    }))
}

fn get_suggested_pools(algorithm: &str) -> Vec<&'static str> {
    match algorithm {
        "kawpow" => vec![
            "stratum+tcp://rvn.2miners.com:6060 - 2Miners",
            "stratum+tcp://rvn-eu1.nanopool.org:12222 - Nanopool",
            "stratum+tcp://rvn.woolypooly.com:55555 - WoolyPooly",
        ],
        "ethash" | "etchash" => vec![
            "stratum+tcp://etc.2miners.com:1010 - 2Miners",
            "stratum+tcp://etc-eu1.nanopool.org:19999 - Nanopool",
            "stratum+tcp://etc.woolypooly.com:35000 - WoolyPooly",
        ],
        "kheavyhash" => vec![
            "stratum+tcp://kas.2miners.com:2020 - 2Miners",
            "stratum+tcp://kas.woolypooly.com:3112 - WoolyPooly",
        ],
        "autolykos2" => vec![
            "stratum+tcp://erg.2miners.com:8888 - 2Miners",
            "stratum+tcp://erg.woolypooly.com:3100 - WoolyPooly",
        ],
        _ => vec![],
    }
}

fn get_default_worker_name() -> String {
    gethostname::gethostname()
        .to_string_lossy()
        .to_string()
}

fn show_summary(config: &MinerConfig) {
    println!("\n{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black());
    println!("{}", "✓ Configuration Complete!".bold().green());
    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black());
    println!();
    println!("  {} {}", "Algorithm:".bold(), config.algorithm.yellow());
    println!("  {} {}", "Pool:".bold(), config.pool.url.cyan());
    println!("  {} {}...{}", "Wallet:".bold(),
        &config.pool.wallet[..8],
        &config.pool.wallet[config.pool.wallet.len()-8..]
    );
    println!("  {} {}", "Worker:".bold(), config.worker_name);
    println!("  {} {} GPU(s)", "Mining with:".bold(), config.gpus.enabled.len());
    println!();
    println!("Configuration saved to: {}", crate::config::get_config_path().display());
}