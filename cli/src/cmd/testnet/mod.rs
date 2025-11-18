//! Testnet commands

use std::path::Path;

use clap::{Parser, Subcommand};
use color_eyre::Result;
use malachitebft_app::node::{CanGeneratePrivateKey, CanMakeGenesis, CanMakePrivateKeyFile, Node};
use malachitebft_config::LoggingConfig;
use malachitebft_core_types::{Context, SigningScheme};

mod generate;
mod init;
pub mod reth;
mod rpc;
mod status;
pub mod types;

pub use generate::{RuntimeFlavour, TestnetConfig, TestnetGenerateCmd};
pub use init::TestnetInitCmd;
pub use reth::check_installation;
pub use status::TestnetStatusCmd;
pub use types::{ProcessHandle, RethNode, RethPorts, TestnetMetadata};

type PrivateKey<C> = <<C as Context>::SigningScheme as SigningScheme>::PrivateKey;

#[derive(Parser, Debug, Clone, PartialEq)]
pub struct TestnetCmd {
    #[command(subcommand)]
    pub command: Option<TestnetSubcommand>,

    /// Fields for backward compatibility (when no subcommand is used)
    #[command(flatten)]
    pub generate_opts: TestnetGenerateCmd,
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum TestnetSubcommand {
    /// Generate testnet configuration (explicit)
    Generate(TestnetGenerateCmd),

    /// Initialize and run a complete testnet with Reth + Emerald nodes
    Init(TestnetInitCmd),

    /// Show status of all nodes in the testnet
    Status(TestnetStatusCmd),
}

impl TestnetCmd {
    /// Execute the testnet command
    pub fn run<N>(&self, node: &N, home_dir: &Path, logging: LoggingConfig) -> Result<()>
    where
        N: Node + CanGeneratePrivateKey + CanMakeGenesis + CanMakePrivateKeyFile,
        PrivateKey<N::Context>: serde::de::DeserializeOwned,
    {
        match &self.command {
            Some(TestnetSubcommand::Generate(cmd)) => cmd.run(node, home_dir, logging),
            Some(TestnetSubcommand::Init(cmd)) => cmd.run(node, home_dir, logging),
            Some(TestnetSubcommand::Status(cmd)) => cmd.run(home_dir),
            // Backward compatibility: if no subcommand, use generate with flattened opts
            None => self.generate_opts.run(node, home_dir, logging),
        }
    }
}
