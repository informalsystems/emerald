use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;
use genesis::{generate_genesis, make_signers};
use spammer::Spammer;

pub mod genesis;
pub mod spammer;
pub mod tx;
pub mod validator_manager;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub async fn run(&self) -> Result<()> {
        match &self.command {
            Commands::Genesis {
                public_keys_file,
                output,
            } => generate_genesis(public_keys_file, output),
            Commands::Spam(spam_cmd) => spam_cmd.run().await,
        }
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Generate genesis file
    Genesis {
        #[clap(short, long)]
        public_keys_file: String,

        #[clap(long, default_value = "./assets/genesis.json")]
        output: String,
    },

    /// Spam transactions
    #[command(arg_required_else_help = true)]
    Spam(SpamCmd),
}

#[derive(Parser, Debug, Clone, Default, PartialEq)]
pub struct SpamCmd {
    /// URL of the execution client's RPC endpoint (e.g., http://127.0.0.1:8545, https://eth.example.com)
    #[clap(long, default_value = "http://127.0.0.1:8545")]
    rpc_url: String,
    /// Number of transactions to send
    #[clap(short, long, default_value = "0")]
    num_txs: u64,
    /// Rate of transactions per second
    #[clap(short, long, default_value = "1000")]
    rate: u64,
    /// Time to run the spammer for in seconds
    #[clap(short, long, default_value = "0")]
    time: u64,
    /// Spam EIP-4844 (blob) transactions instead of EIP-1559
    #[clap(long, default_value = "false")]
    blobs: bool,

    #[clap(long, default_value = "0")]
    signer_index: usize,
}

impl SpamCmd {
    pub(crate) async fn run(&self) -> Result<()> {
        let SpamCmd {
            rpc_url,
            num_txs,
            rate,
            time,
            blobs,
            signer_index,
        } = self;

        // check if url contains valid http/https scheme
        let url = if rpc_url.starts_with("http://") || rpc_url.starts_with("https://") {
            rpc_url.clone().parse::<url::Url>()?
        } else {
            return Err(color_eyre::eyre::eyre!(
                "Invalid RPC URL scheme. Please provide a valid http:// or https:// URL."
            ));
        };

        Spammer::new(url, *signer_index, *num_txs, *time, *rate, *blobs)?
            .run()
            .await
    }
}
