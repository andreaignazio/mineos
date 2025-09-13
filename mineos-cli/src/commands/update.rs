use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct UpdateArgs {
    #[arg(long)]
    check: bool,
}

pub async fn execute(_args: UpdateArgs) -> Result<()> {
    println!("Update checker - coming soon");
    Ok(())
}