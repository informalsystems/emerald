use clap::Parser;
use color_eyre::eyre::Result;
use malachitebft_eth_utils::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

    Cli::parse().run().await
}
