use alloy_primitives::Address;
use clap::{Parser, Subcommand, ValueHint};
use color_eyre::eyre::Result;
use genesis::{generate_genesis, make_signers};
use mempool_monitor::MempoolMonitor;
use reqwest::Url;
use spammer::Spammer;

pub mod dex_templates;
pub mod dexalot_generator;
pub mod genesis;
pub mod mempool_monitor;
pub mod modify_config;
pub mod poa;
pub mod rubicon_generator;
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
                poa_owner_address,
                devnet,
                devnet_balance,
                chain_id,
                evm_genesis_output,
                emerald_genesis_output,
            } => generate_genesis(
                public_keys_file,
                poa_owner_address,
                devnet,
                devnet_balance,
                chain_id,
                evm_genesis_output,
                emerald_genesis_output,
            ),
            Commands::Spam(spam_cmd) => spam_cmd.run().await,
            Commands::Poa(poa_cmd) => poa_cmd.run().await,
            Commands::SpamContract(spam_contract_cmd) => spam_contract_cmd.run().await,
            Commands::MonitorMempool(monitor_cmd) => monitor_cmd.run().await,
            Commands::ModifyConfig(modify_config_cmd) => modify_config_cmd.run(),
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
            value_hint = ValueHint::FilePath,
            help = "File containing validator public keys (one per line)"
        )]
        public_keys_file: String,

        #[clap(
            long,
            short = 'a',
            required_unless_present = "devnet",
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
        devnet: bool,

        #[clap(
            long,
            short = 'b',
            default_value_t = 15_000_u64,
            help = "Balance for each testnet wallet (default: 15000)"
        )]
        devnet_balance: u64,

        #[clap(
            long,
            short = 'g',
            value_hint = ValueHint::FilePath,
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

    #[command(arg_required_else_help = true)]
    Poa(PoaCmd),

    /// Spam contract transactions
    #[command(arg_required_else_help = true)]
    SpamContract(SpamContractCmd),

    /// Monitor mempool and log when it becomes empty
    #[command(arg_required_else_help = true)]
    MonitorMempool(MonitorMempoolCmd),

    /// Apply custom node configurations from a TOML file
    #[command(arg_required_else_help = true)]
    ModifyConfig(ModifyConfigCmd),
}

#[derive(Parser, Debug, Clone, Default, PartialEq)]
pub struct SpamCmd {
    /// URL of the execution client's RPC endpoint (e.g., http://127.0.0.1:8545, https://eth.example.com)
    #[clap(long, default_value = "http://127.0.0.1:8545")]
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
    /// Interval in ms for sending batches of transactions
    #[clap(short, long, default_value = "200")]
    interval: u64,
    /// Time to run the spammer for in seconds
    #[clap(short, long, default_value = "0")]
    time: u64,
    /// Spam EIP-4844 (blob) transactions instead of EIP-1559
    #[clap(long, default_value = "false")]
    blobs: bool,

    #[clap(long, default_value = "0")]
    signer_index: usize,

    /// Dexalot Portfolio contract address (required for dexalot templates)
    #[clap(long, required_if_eq("template", "dexalot"))]
    portfolio: Option<Address>,

    /// Dexalot TradePairs contract address (required for dexalot templates)
    #[clap(long, required_if_eq("template", "dexalot"))]
    tradepairs: Option<Address>,

    /// RubiconMarket contract address (for rubicon templates)
    #[clap(long)]
    rubicon_market: Option<Address>,

    /// WETH9 token contract address (for rubicon templates)
    #[clap(long)]
    weth: Option<Address>,

    /// USDC token contract address (for rubicon templates)
    #[clap(long)]
    usdc: Option<Address>,

    #[clap(long, short)]
    chain_id: u64,
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
        let Self {
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
        let Self {
            rpc_url,
            dex,
            template,
            num_txs,
            rate,
            interval,
            time,
            blobs,
            signer_index,
            portfolio,
            tradepairs,
            rubicon_market,
            weth,
            usdc,
            chain_id,
        } = self;

        let url: Url = if rpc_url.starts_with("http://") || rpc_url.starts_with("https://") {
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

            // Check if this is a dexalot template (generate dynamically)
            if config_path.contains("dexalot") {
                // Get signer address for dynamic template generation
                let signers = make_signers();
                let signer_address = signers[*signer_index].address();

                // Require both portfolio and tradepairs addresses
                let portfolio_addr = portfolio.ok_or_else(|| {
                    color_eyre::eyre::eyre!(
                        "Dexalot template requires --portfolio address to be specified"
                    )
                })?;
                let tradepairs_addr = tradepairs.ok_or_else(|| {
                    color_eyre::eyre::eyre!(
                        "Dexalot template requires --tradepairs address to be specified"
                    )
                })?;

                let config = dexalot_generator::DexalotConfig {
                    portfolio: portfolio_addr,
                    tradepairs: tradepairs_addr,
                };

                let templates =
                    dexalot_generator::generate_dexalot_transactions(signer_address, config)?;

                // Save generated templates to file
                let output_path = "utils/examples/generated_dexalot.yaml";
                if let Err(e) = dex_templates::save_templates(&templates, output_path) {
                    eprintln!("Warning: Failed to save generated template: {e}");
                } else {
                    println!("Generated Dexalot template saved to: {output_path}");
                }

                Some(templates)
            } else if config_path.contains("rubicon") || rubicon_market.is_some() {
                // Check if this is a rubicon template (generate dynamically)
                // Require all three contract addresses
                let rubicon_market_addr = rubicon_market.ok_or_else(|| {
                    color_eyre::eyre::eyre!(
                        "Rubicon template requires --rubicon-market address to be specified"
                    )
                })?;
                let weth_addr = weth.ok_or_else(|| {
                    color_eyre::eyre::eyre!(
                        "Rubicon template requires --weth address to be specified"
                    )
                })?;
                let usdc_addr = usdc.ok_or_else(|| {
                    color_eyre::eyre::eyre!(
                        "Rubicon template requires --usdc address to be specified"
                    )
                })?;

                let config = rubicon_generator::RubiconConfig {
                    rubicon_market: rubicon_market_addr,
                    weth: weth_addr,
                    usdc: usdc_addr,
                };

                println!("Generating Rubicon DEX transaction templates with custom addresses");
                let templates = rubicon_generator::generate_rubicon_transactions(config)?;

                // Save generated templates to file
                let output_path = "utils/examples/generated_rubicon.yaml";
                if let Err(e) = dex_templates::save_templates(&templates, output_path) {
                    eprintln!("Warning: Failed to save generated template: {e}");
                } else {
                    println!("Generated Rubicon template saved to: {output_path}");
                }

                Some(templates)
            } else {
                println!("Loading DEX transaction templates from: {config_path}");
                Some(dex_templates::load_templates(config_path)?)
            }
        } else if template.is_some() {
            // If template is specified but --dex is not set, warn the user
            eprintln!(
                "Warning: --template specified without --dex flag. Template will be ignored."
            );
            None
        } else {
            None
        };

        let config = spammer::SpammerConfig {
            max_num_txs: *num_txs,
            max_time: *time,
            max_rate: *rate,
            batch_interval: *interval,
            blobs: *blobs,
            chain_id: *chain_id,
        };

        Spammer::new(url, *signer_index, config, templates)?.run().await
    }
}

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct PoaCmd {
    /// RPC URL
    #[clap(long, short, default_value = "http://127.0.0.1:8545")]
    rpc_url: Url,

    /// ValidatorManager contract address
    #[clap(
        long,
        short,
        default_value_t = alloy_primitives::address!("0x0000000000000000000000000000000000002000")
    )]
    contract_address: alloy_primitives::Address,

    #[command(subcommand)]
    command: PoaCommands,
}

impl PoaCmd {
    pub async fn run(&self) -> Result<()> {
        match &self.command {
            PoaCommands::AddValidator {
                validator_pubkey,
                power,
                owner_private_key,
            } => {
                let url = &self.rpc_url;
                let address = &self.contract_address;
                poa::add_validator(url, address, validator_pubkey, *power, owner_private_key).await
            }
            PoaCommands::RemoveValidator {
                validator_identifier,
                owner_private_key,
            } => {
                let url = &self.rpc_url;
                let address = &self.contract_address;
                poa::remove_validator(url, address, validator_identifier, owner_private_key).await
            }
            PoaCommands::List {} => {
                let url = &self.rpc_url;
                let address = &self.contract_address;
                poa::list_validators(url, address).await
            }
            PoaCommands::UpdateValidator {
                validator_identifier,
                power,
                owner_private_key,
            } => {
                let url = &self.rpc_url;
                let address = &self.contract_address;
                poa::update_validator_power(
                    url,
                    address,
                    validator_identifier,
                    *power,
                    owner_private_key,
                )
                .await
            }
        }
    }
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum PoaCommands {
    /// Add a validator
    AddValidator {
        /// Validator public key (128-130 hex chars) or address (40 hex chars)
        #[clap(long, short = 'v')]
        validator_pubkey: String,

        /// Validator power (voting weight)
        #[clap(long, short, default_value_t = 100)]
        power: u64,

        /// Private key of the contract owner
        #[clap(long, short)]
        owner_private_key: String,
    },
    /// Remove a validator
    RemoveValidator {
        /// Validator public key (128-130 hex chars) or address (40 hex chars)
        #[clap(long, short = 'v')]
        validator_identifier: String,

        /// Private key of the contract owner
        #[clap(long, short)]
        owner_private_key: String,
    },
    UpdateValidator {
        /// Validator public key (128-130 hex chars) or address (40 hex chars)
        #[clap(long, short = 'v')]
        validator_identifier: String,

        /// New validator power (voting weight)
        #[clap(long, short, default_value_t = 100)]
        power: u64,

        /// Private key of the contract owner
        #[clap(long, short)]
        owner_private_key: String,
    },
    List {},
}

#[derive(Parser, Debug, Clone, Default, PartialEq)]
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
    #[clap(long, default_value = "127.0.0.1:8645")]
    rpc_url: String,
    /// Number of transactions to send
    #[clap(short, long, default_value_t = 0)]
    num_txs: u64,
    /// Rate of transactions per second
    #[clap(short, long, default_value_t = 1000)]
    rate: u64,
    /// Interval in ms for sending batches of transactions
    #[clap(short, long, default_value = "200")]
    interval: u64,
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
            interval,
            time,
            signer_index,
            chain_id,
        } = self;
        let url = format!("http://{rpc_url}").parse()?;
        let config = spammer::SpammerConfig {
            max_num_txs: *num_txs,
            max_time: *time,
            max_rate: *rate,
            batch_interval: *interval,
            blobs: false,
            chain_id: *chain_id,
        };
        Spammer::new_contract(url, *signer_index, config, contract, function, args)?
            .run()
            .await
    }
}

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct ModifyConfigCmd {
    /// Path to the directory containing node configurations (e.g., 'nodes')
    #[clap(long, value_hint = ValueHint::DirPath)]
    node_config_home: std::path::PathBuf,

    /// Path to the custom TOML configuration file (e.g., 'assets/emerald_p2p_config.toml')
    #[clap(long, value_hint = ValueHint::FilePath)]
    custom_config_file_path: std::path::PathBuf,
}

impl ModifyConfigCmd {
    pub fn run(&self) -> Result<()> {
        modify_config::apply_custom_config(&self.node_config_home, &self.custom_config_file_path)
    }
}
