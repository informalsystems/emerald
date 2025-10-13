use chrono::Local;
use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;
use dex_templates::load_templates;
use genesis::{generate_genesis, make_signers};
use mempool_monitor::MempoolMonitor;
use spammer::Spammer;
use std::fs::{self, File};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod dex_templates;
mod genesis;
mod mempool_monitor;
mod spammer;
mod tx;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Generate genesis file
    Genesis,

    /// Spam transactions
    #[command(arg_required_else_help = true)]
    Spam(SpamCmd),

    /// Monitor mempool and log when it becomes empty
    #[command(arg_required_else_help = true)]
    MonitorMempool(MonitorMempoolCmd),
}

#[derive(Parser, Debug, Clone, Default, PartialEq)]
pub struct SpamCmd {
    /// URL of the execution client's RPC endpoint
    #[clap(long, default_value = "127.0.0.1:8545")]
    rpc_url: String,

    /// Enable DEX transaction spamming mode (loads exchange_transactions.yaml)
    #[clap(long, default_value = "false")]
    dex: bool,

    /// Path to custom transaction template YAML file (used with --dex)
    #[clap(long)]
    template: Option<String>,

    /// Number of transactions to send
    #[clap(short, long, default_value = "0")]
    num_txs: u64,
    /// Rate of transactions per second
    #[clap(short, long, default_value = "1000")]
    rate: u64,
    /// Time to run the spammer for in seconds
    #[clap(short, long, default_value = "0")]
    time: u64,
    /// Spam EIP-4844 (blob) transactions instead of EIP-1559 (ignored when --dex is true)
    #[clap(long, default_value = "false")]
    blobs: bool,

    #[clap(long, default_value = "0")]
    signer_index: usize,
}

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct MonitorMempoolCmd {
    /// URL of the execution client's RPC endpoint
    #[clap(long, default_value = "127.0.0.1:8545")]
    rpc_url: String,

    /// Polling interval in milliseconds (lower = more precise, higher CPU usage)
    #[clap(long, default_value = "10")]
    poll_interval_ms: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();
    match cli.command {
        Commands::Genesis => generate_genesis(),
        Commands::MonitorMempool(MonitorMempoolCmd {
            rpc_url,
            poll_interval_ms,
        }) => {
            // Create logs directory if it doesn't exist
            let log_dir = "./utils/logs";
            fs::create_dir_all(log_dir)?;

            // Create a new log file with timestamp
            let timestamp = Local::now().format("%Y%m%d_%H%M%S");
            let log_file_path = format!("{}/mempool_monitor_{}.log", log_dir, timestamp);
            let log_file = File::create(&log_file_path)?;

            println!("Logging to: {}", log_file_path);

            // Initialize tracing subscriber with file output
            tracing_subscriber::registry()
                .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
                .with(fmt::layer().with_writer(log_file).with_ansi(false))
                .init();

            let url = format!("http://{rpc_url}").parse()?;
            MempoolMonitor::new(url, poll_interval_ms).run().await
        }
        Commands::Spam(SpamCmd {
            rpc_url,
            dex,
            template,
            num_txs,
            rate,
            time,
            blobs,
            signer_index,
        }) => {
            let url = format!("http://{rpc_url}").parse()?;

            // Load DEX templates if --dex flag is set
            let templates = if dex {
                // Create logs directory if it doesn't exist
                let log_dir = "./utils/logs";
                fs::create_dir_all(log_dir)?;

                // Create a new log file with timestamp
                let timestamp = Local::now().format("%Y%m%d_%H%M%S");
                let log_file_path = format!("{}/dex_spammer{}.log", log_dir, timestamp);
                let log_file = File::create(&log_file_path)?;

                println!("Logging to: {}", log_file_path);

                // Initialize tracing subscriber with file output
                tracing_subscriber::registry()
                    .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
                    .with(fmt::layer().with_writer(log_file).with_ansi(false))
                    .init();
                // Use custom template path if provided, otherwise use default
                let config_path = template
                    .as_deref()
                    .unwrap_or("utils/examples/exchange_transactions.yaml");
                println!("Loading DEX transaction templates from: {}", config_path);
                Some(load_templates(config_path)?)
            } else if template.is_some() {
                // If template is specified but --dex is not set, warn the user
                eprintln!(
                    "Warning: --template specified without --dex flag. Template will be ignored."
                );
                None
            } else {
                None
            };

            Spammer::new(url, signer_index, num_txs, time, rate, blobs, templates)?
                .run()
                .await
        }
    }
}
