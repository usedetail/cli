use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    detail_cli::Cli::parse().run().await
}
