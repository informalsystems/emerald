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
            Commands::SpamContract(spam_contract_cmd) => spam_contract_cmd.run().await,
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

    /// Spam contract transactions
    #[command(arg_required_else_help = true)]
    SpamContract(SpamContractCmd),
}

#[derive(Parser, Debug, Clone, Default, PartialEq)]
pub struct SpamCmd {
    /// URL of the execution client's RPC endpoint
    #[clap(long, default_value = "127.0.0.1:8545")]
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
        let url = format!("http://{rpc_url}").parse()?;
        Spammer::new(url, *signer_index, *num_txs, *time, *rate, *blobs)?
            .run()
            .await
    }
}

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct SpamContractCmd {
    /// Contract address to spam
    #[clap(long)]
    contract: String,
    /// Function signature (e.g., "increment()" or "setNumber(uint256)")
    #[clap(long)]
    function: String,
    /// Optional function arguments (comma-separated, e.g., "42" or "100,0x...")
    #[clap(long, default_value = "")]
    args: String,
    /// URL of the execution client's RPC endpoint
    #[clap(long, default_value = "127.0.0.1:8545")]
    rpc_url: String,
    /// Number of transactions to send
    #[clap(short, long, default_value_t = 0)]
    num_txs: u64,
    /// Rate of transactions per second
    #[clap(short, long, default_value_t = 1000)]
    rate: u64,
    /// Time to run the spammer for in seconds
    #[clap(short, long, default_value_t = 0)]
    time: u64,
    #[clap(long, default_value_t = 0)]
    signer_index: usize,
}

impl SpamContractCmd {
    pub(crate) async fn run(&self) -> Result<()> {
        let SpamContractCmd {
            contract,
            function,
            args,
            rpc_url,
            num_txs,
            rate,
            time,
            signer_index,
        } = self;
        let url = format!("http://{rpc_url}").parse()?;
        Spammer::new_contract(
            url,
            *signer_index,
            *num_txs,
            *time,
            *rate,
            contract,
            function,
            args,
        )?
        .run()
        .await
    }
}
