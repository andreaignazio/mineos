use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct OverclockArgs {
    #[arg(long)]
    gpu: Option<usize>,
    #[arg(long)]
    core: Option<i32>,
    #[arg(long)]
    mem: Option<i32>,
}

pub async fn execute(_args: OverclockArgs) -> Result<()> {
    println!("Overclocking control - coming soon");
    Ok(())
}