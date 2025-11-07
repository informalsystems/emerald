use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;
use genesis::{generate_genesis, make_signers};
use spammer::Spammer;

pub mod genesis;
pub mod poa;
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
            Commands::Poa(poa_cmd) => poa_cmd.run().await,
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

    #[command(arg_required_else_help = true)]
    Poa(PoaCmd),
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
pub struct PoaCmd {
    #[command(subcommand)]
    command: PoaCommands,
}

impl PoaCmd {
    pub async fn run(&self) -> Result<()> {
        match &self.command {
            PoaCommands::AddValidator {
                validator_pubkey,
                rpc_url,
                contract_address,
                power,
                owner_private_key,
            } => {
                let url = rpc_url.parse()?;
                let address = contract_address.parse()?;
                poa::add_validator(url, address, validator_pubkey, *power, owner_private_key).await
            }
            PoaCommands::RemoveValidator {
                validator_pubkey,
                rpc_url,
                contract_address,
                owner_private_key,
            } => {
                let url = rpc_url.parse()?;
                let address = contract_address.parse()?;
                poa::remove_validator(url, address, validator_pubkey, owner_private_key).await
            }
            PoaCommands::List {
                rpc_url,
                contract_address,
            } => {
                let url = rpc_url.parse()?;
                let address = contract_address.parse()?;
                poa::list_validators(url, address).await
            }
        }
    }
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum PoaCommands {
    /// Add a validator
    AddValidator {
        /// Validator public key (uncompressed secp256k1, hex encoded)
        #[clap(long, short)]
        validator_pubkey: String,

        /// RPC URL
        #[clap(long, short, default_value = "http://127.0.0.1:8545")]
        rpc_url: String,

        /// ValidatorManager contract address
        #[clap(long, short)]
        contract_address: String,

        /// Validator power (voting weight)
        #[clap(long, short, default_value = "1")]
        power: u64,

        /// Private key of the contract owner
        #[clap(long, short, env = "owner_private_key")]
        owner_private_key: String,
    },
    /// Remove a validator
    RemoveValidator {
        /// Validator public key (uncompressed secp256k1, hex encoded)
        #[clap(long, short)]
        validator_pubkey: String,

        /// RPC URL
        #[clap(long, short, default_value = "http://127.0.0.1:8545")]
        rpc_url: String,

        /// ValidatorManager contract address
        #[clap(long, short)]
        contract_address: String,

        /// Private key of the contract owner
        #[clap(long, short, env = "owner_private_key")]
        owner_private_key: String,
    },
    List {
        /// RPC URL
        #[clap(short = 'r', long, default_value = "http://127.0.0.1:8545")]
        rpc_url: String,

        /// ValidatorManager contract address
        #[clap(short = 'c', long)]
        contract_address: String,
    },
}
