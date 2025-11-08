use alloy_primitives::Address;
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
                poa_owner_address,
                testnet,
                testnet_balance,
                chain_id,
                evm_genesis_output,
                emerald_genesis_output,
            } => generate_genesis(
                public_keys_file,
                poa_owner_address,
                testnet,
                testnet_balance,
                chain_id,
                evm_genesis_output,
                emerald_genesis_output,
            ),
            Commands::Spam(spam_cmd) => spam_cmd.run().await,
            Commands::SpamContract(spam_contract_cmd) => spam_contract_cmd.run().await,
        }
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Generate genesis file
    Genesis {
        #[clap(
            short,
            long,
            help = "File containing validator public keys (one per line)"
        )]
        public_keys_file: String,

        #[clap(
            long,
            short = 'a',
            required_unless_present = "testnet",
            help = "Address of the Proof-of-Authority owner"
        )]
        poa_owner_address: Option<String>,

        #[clap(
            long,
            short = 'c',
            help = "Chain ID for the genesis file (default: 12345)",
            default_value_t = 12345
        )]
        chain_id: u64,

        #[clap(
            short,
            long,
            default_value_t = false,
            help = "Generate test addresses in genesis using mnemonic: 'test test test test test test test test test test test junk'"
        )]
        testnet: bool,

        #[clap(
            long,
            short = 'b',
            default_value_t = 15_000u64,
            help = "Balance for each testnet wallet (default: 15000)"
        )]
        testnet_balance: u64,

        #[clap(
            long,
            short = 'g',
            default_value = "./assets/genesis.json",
            help = "Output path for the generated genesis file"
        )]
        evm_genesis_output: String,

        #[clap(
            long,
            short = 'e',
            default_value = "./assets/emerald_genesis.json",
            help = "Output path for the generated Emerald genesis file"
        )]
        emerald_genesis_output: String,
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

    #[clap(long, short)]
    chain_id: u64,
}

impl SpamCmd {
    pub(crate) async fn run(&self) -> Result<()> {
        let Self {
            rpc_url,
            num_txs,
            rate,
            time,
            blobs,
            signer_index,
            chain_id,
        } = self;
        let url = format!("http://{rpc_url}").parse()?;
        Spammer::new(
            url,
            *signer_index,
            *num_txs,
            *time,
            *rate,
            *blobs,
            *chain_id,
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
    #[clap(long, short)]
    chain_id: u64,
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
            chain_id,
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
            *chain_id,
        )?
        .run()
        .await
    }
}
