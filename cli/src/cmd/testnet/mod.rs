//! Testnet commands

use std::path::Path;

use clap::{Parser, Subcommand};
use color_eyre::Result;
use malachitebft_app::node::{CanGeneratePrivateKey, CanMakeGenesis, CanMakePrivateKeyFile, Node};
use malachitebft_config::LoggingConfig;
use malachitebft_core_types::{Context, SigningScheme};

mod add_node;
pub mod config;
mod generate;
pub mod reth;
mod rm;
mod rpc;
mod start;
mod start_node;
mod status;
mod stop;
mod stop_node;
pub mod types;

pub use add_node::TestnetAddNodeCmd;
pub use generate::{RuntimeFlavour, TestnetConfig, TestnetGenerateCmd};
pub use reth::check_installation;
pub use rm::TestnetRmCmd;
pub use start::TestnetStartCmd;
pub use start_node::TestnetStartNodeCmd;
pub use status::TestnetStatusCmd;
pub use stop::TestnetStopCmd;
pub use stop_node::TestnetStopNodeCmd;
pub use types::{ProcessHandle, RethNode, RethPorts};

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

    /// Start a complete testnet with Reth + Emerald nodes
    Start(TestnetStartCmd),

    /// Show status of all nodes in the testnet
    Status(TestnetStatusCmd),

    /// Add a new node to an existing testnet
    AddNode(TestnetAddNodeCmd),

    /// Restart an existing stopped node by ID
    StartNode(TestnetStartNodeCmd),

    /// Stop an existing running node by ID
    StopNode(TestnetStopNodeCmd),

    /// Stop all nodes in the testnet
    Stop(TestnetStopCmd),

    /// Remove all testnet data
    Rm(TestnetRmCmd),
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
            Some(TestnetSubcommand::Start(cmd)) => cmd.run(node, home_dir, logging),
            Some(TestnetSubcommand::Status(cmd)) => cmd.run(home_dir),
            Some(TestnetSubcommand::AddNode(cmd)) => cmd.run(home_dir),
            Some(TestnetSubcommand::StartNode(cmd)) => cmd.run(home_dir),
            Some(TestnetSubcommand::StopNode(cmd)) => cmd.run(home_dir),
            Some(TestnetSubcommand::Stop(cmd)) => cmd.run(home_dir),
            Some(TestnetSubcommand::Rm(cmd)) => cmd.run(home_dir),
            // Backward compatibility: if no subcommand, use generate with flattened opts
            None => self.generate_opts.run(node, home_dir, logging),
        }
    }
}
