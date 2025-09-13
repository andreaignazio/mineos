use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct ConfigArgs {
    #[arg(long)]
    show: bool,
    #[arg(long)]
    edit: bool,
}

pub async fn execute(_args: ConfigArgs) -> Result<()> {
    println!("Config management - coming soon");
    Ok(())
}