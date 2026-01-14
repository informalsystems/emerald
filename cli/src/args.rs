//! Command-line interface arguments for a basic implementation.
//!
//! Read configuration from the configuration files found in the directory
//! provided with the `--home` global parameter.
//!
//! The command-line parameters are stored in the `Args` structure.
//! `clap` parses the command-line parameters into this structure.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use directories::BaseDirs;
use malachitebft_config::{LogFormat, LogLevel};

use crate::cmd::distributed_testnet::DistributedTestnetCmd;
use crate::cmd::init::InitCmd;
use crate::cmd::show_pubkey::ShowPubkeyCmd;
use crate::cmd::start::StartCmd;
use crate::cmd::testnet::TestnetCmd;
use crate::error::Error;

const EMERALD_FOLDER: &str = ".emerald";
const CONFIG_FILE: &str = "config.toml";
const GENESIS_FILE: &str = "genesis.json";
const PRIV_VALIDATOR_KEY_FILE: &str = "priv_validator_key.json";

#[derive(Parser, Clone, Debug, Default)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Home directory for Malachite (default: `$HOME/.emerald-devnet`)
    #[arg(long, global = true, value_name = "HOME_DIR")]
    pub home: Option<PathBuf>,

    /// Log level (default: `malachite=debug`)
    #[arg(long, global = true, value_name = "LOG_LEVEL")]
    pub log_level: Option<LogLevel>,

    /// Log format (default: `plaintext`)
    #[arg(long, global = true, value_name = "LOG_FORMAT")]
    pub log_format: Option<LogFormat>,

    /// Emerald configuration file (default: `~/.emerald/config/config.toml`)
    #[arg(long, global = true, value_name = "CONFIG_FILE")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Clone, Debug)]
pub enum Commands {
    /// Start node
    Start(StartCmd),

    /// Initialize configuration
    Init(InitCmd),

    /// Generate testnet configuration
    Testnet(TestnetCmd),

    /// Generate distributed testnet configuration
    DistributedTestnet(DistributedTestnetCmd),

    /// Extract secp256k1 public key from a file containing a Secp256k1 private key
    ShowPubkey(ShowPubkeyCmd),
}

impl Default for Commands {
    fn default() -> Self {
        Self::Start(StartCmd::default())
    }
}

impl Args {
    /// new returns a new instance of the arguments.
    pub fn new() -> Self {
        Self::parse()
    }

    /// get_home_dir returns the application home folder.
    /// Defaults to `$HOME/.emerald-devnet`.
    pub fn get_home_dir(&self) -> Result<PathBuf, Error> {
        match self.home {
            Some(ref path) => Ok(path.clone()),
            None => Ok(BaseDirs::new()
                .ok_or(Error::DirPath)?
                .home_dir()
                .join(".emerald-devnet")),
        }
    }

    /// get_emerald_config_file returns the application configuration file.
    /// Typically, `$HOME/.emerald/config/config.toml`.
    pub fn get_emerald_config_file(&self) -> Result<PathBuf, Error> {
        match self.config {
            Some(ref path) => Ok(path.clone()),
            None => Ok(BaseDirs::new()
                .ok_or(Error::DirPath)?
                .home_dir()
                .join(EMERALD_FOLDER)
                .join("config")
                .join("config.toml")),
        }
    }

    /// get_config_dir returns the configuration folder based on the home folder.
    pub fn get_config_dir(&self) -> Result<PathBuf, Error> {
        Ok(self.get_home_dir()?.join("config"))
    }

    /// get_config_file_path returns the configuration file path based on the command-line arguments
    /// and the configuration folder.
    pub fn get_config_file_path(&self) -> Result<PathBuf, Error> {
        Ok(self.get_config_dir()?.join(CONFIG_FILE))
    }

    /// get_genesis_file_path returns the genesis file path based on the command-line arguments and
    /// the configuration folder.
    pub fn get_genesis_file_path(&self) -> Result<PathBuf, Error> {
        Ok(self.get_config_dir()?.join(GENESIS_FILE))
    }

    /// get_log_level_or_default returns the log level from the command-line or the default value.
    pub fn get_log_level_or_default(&self) -> LogLevel {
        self.log_level.unwrap_or_default()
    }

    /// get_priv_validator_key_file_path returns the private validator key file path based on the
    /// configuration folder.
    pub fn get_priv_validator_key_file_path(&self) -> Result<PathBuf, Error> {
        Ok(self.get_config_dir()?.join(PRIV_VALIDATOR_KEY_FILE))
    }
}
