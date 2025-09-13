use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct ProfitArgs {
    #[arg(long)]
    power_cost: Option<f64>,
}

pub async fn execute(_args: ProfitArgs) -> Result<()> {
    println!("Profit calculator - coming soon");
    Ok(())
}