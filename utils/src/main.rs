use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;
use dex_templates::load_templates;
use genesis::{generate_genesis, make_signers};
use spammer::Spammer;

mod dex_templates;
mod genesis;
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

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();
    match cli.command {
        Commands::Genesis => generate_genesis(),
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
