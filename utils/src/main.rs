use std::fs::{self, File};

use chrono::Local;
use clap::Parser;
use color_eyre::eyre::Result;
use emerald_utils::{Cli, Commands};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    // Initialize tracing based on command type
    match &cli.command {
        Commands::Spam(spam_cmd) if spam_cmd.dex => {
            // Create logs directory
            let log_dir = "./utils/logs";
            fs::create_dir_all(log_dir)?;

            // Create log file with timestamp
            let timestamp = Local::now().format("%Y%m%d_%H%M%S");
            let log_file_path = format!("{log_dir}/dex_spammer_{timestamp}.log");
            let log_file = File::create(&log_file_path)?;

            println!("Logging to: {log_file_path}");

            // Initialize file-based logger
            tracing_subscriber::registry()
                .with(EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
                .with(fmt::layer().with_writer(log_file).with_ansi(false))
                .init();
        }
        Commands::MonitorMempool(_) => {
            // Create logs directory
            let log_dir = "./utils/logs";
            fs::create_dir_all(log_dir)?;

            // Create log file with timestamp
            let timestamp = Local::now().format("%Y%m%d_%H%M%S");
            let log_file_path = format!("{log_dir}/mempool_monitor_{timestamp}.log");
            let log_file = File::create(&log_file_path)?;

            println!("Logging to: {log_file_path}");

            // Initialize file-based logger
            tracing_subscriber::registry()
                .with(EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
                .with(fmt::layer().with_writer(log_file).with_ansi(false))
                .init();
        }
        _ => {
            // Default console logger for other commands
            tracing_subscriber::fmt::init();
        }
    }

    cli.run().await
}
