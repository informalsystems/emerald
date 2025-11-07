use alloy_primitives::Address;
use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;
use genesis::{generate_genesis, make_signers};
use mempool_monitor::MempoolMonitor;
use spammer::Spammer;

pub mod dex_templates;
pub mod genesis;
pub mod mempool_monitor;
pub mod spammer;
pub mod tx;
pub mod validator_manager;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
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
            Commands::MonitorMempool(monitor_cmd) => monitor_cmd.run().await,
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
    pub dex: bool,
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
    /// Spam EIP-4844 (blob) transactions instead of EIP-1559
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

impl MonitorMempoolCmd {
    pub(crate) async fn run(&self) -> Result<()> {
        let MonitorMempoolCmd {
            rpc_url,
            poll_interval_ms,
        } = self;

        let url = if rpc_url.starts_with("http://") || rpc_url.starts_with("https://") {
            rpc_url.parse()?
        } else {
            format!("http://{rpc_url}").parse()?
        };
        MempoolMonitor::new(url, *poll_interval_ms).run().await
    }
}
impl SpamCmd {
    pub(crate) async fn run(&self) -> Result<()> {
        let SpamCmd {
            rpc_url,
            dex,
            template,
            num_txs,
            rate,
            time,
            blobs,
            signer_index,
        } = self;
        let url = if rpc_url.starts_with("http://") || rpc_url.starts_with("https://") {
            rpc_url.parse()?
        } else {
            format!("http://{rpc_url}").parse()?
        };

        // Load DEX templates if --dex flag is set
        let templates = if *dex {
            // Use custom template path if provided, otherwise use default
            let config_path = template
                .as_deref()
                .unwrap_or("utils/examples/exchange_transactions.yaml");
            println!("Loading DEX transaction templates from: {config_path}");
            Some(dex_templates::load_templates(config_path)?)
        } else if template.is_some() {
            // If template is specified but --dex is not set, warn the user
            eprintln!(
                "Warning: --template specified without --dex flag. Template will be ignored."
            );
            None
        } else {
            None
        };

        Spammer::new(
            url,
            *signer_index,
            *num_txs,
            *time,
            *rate,
            *blobs,
            templates,
        )?
        .run()
        .await
    }
}

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct SpamContractCmd {
    /// Contract address to spam
    #[clap(long)]
    contract: Address,
    /// Function signature (e.g., "increment()" or "setNumber(uint256)")
    #[clap(long)]
    function: String,
    /// Function arguments (supply multiple `--args` or a comma-separated list. e.g. "42" or "42,0x...")
    #[clap(long, value_delimiter = ',', num_args = 0..)]
    args: Vec<String>,
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
        let Self {
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
