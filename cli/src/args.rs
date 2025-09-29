//! Command-line interface arguments for a basic implementation.
//!
//! Read configuration from the configuration files found in the directory
//! provided with the `--home` global parameter.
//!
//! The command-line parameters are stored in the `Args` structure.
//! `clap` parses the command-line parameters into this structure.

use std::path::PathBuf;
use std::str::FromStr;

use clap::{Parser, Subcommand};
use directories::BaseDirs;

use malachitebft_config::{LogFormat, LogLevel};

use crate::cmd::distributed_testnet::DistributedTestnetCmd;
use crate::cmd::init::InitCmd;
use crate::cmd::start::StartCmd;
use crate::cmd::testnet::TestnetCmd;
use crate::error::Error;

const APP_FOLDER: &str = ".malachite";
const CONFIG_FILE: &str = "config.toml";
const GENESIS_FILE: &str = "genesis.json";
const PRIV_VALIDATOR_KEY_FILE: &str = "priv_validator_key.json";
#[derive(Parser, Clone, Debug, Default)]
pub struct EthArgs {
    /// Path to JWT secret file for Ethereum engine API authentication
    #[arg(long, value_name = "JWT_SECRET_PATH", global = true)]
    pub jwt_secret_path: Option<PathBuf>,

    /// URL for Ethereum engine RPC endpoint
    #[arg(long, value_name = "ENGINE_RPC_URL", global = true)]
    pub engine_rpc_url: Option<String>,

    /// URL for Ethereum RPC endpoint                                                                                                                                                               
    #[arg(long, value_name = "ETHERIUM_RPC_URL", global = true)]
    pub etherium_rpc_url: Option<String>,
}

#[derive(Parser, Clone, Debug, Default)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Home directory for Malachite (default: `~/.malachite`)
    #[arg(long, global = true, value_name = "HOME_DIR")]
    pub home: Option<PathBuf>,

    /// Log level (default: `malachite=debug`)
    #[arg(long, global = true, value_name = "LOG_LEVEL")]
    pub log_level: Option<LogLevel>,

    /// Log format (default: `plaintext`)
    #[arg(long, global = true, value_name = "LOG_FORMAT")]
    pub log_format: Option<LogFormat>,

    #[command(subcommand)]
    pub command: Commands,

    #[command(flatten)]
    pub eth: EthArgs,
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
}

impl Default for Commands {
    fn default() -> Self {
        Commands::Start(StartCmd::default())
    }
}

impl Args {
    /// new returns a new instance of the arguments.
    pub fn new() -> Args {
        Args::parse()
    }

    /// get_home_dir returns the application home folder.
    /// Typically, `$HOME/.malachite`, dependent on the operating system.
    pub fn get_home_dir(&self) -> Result<PathBuf, Error> {
        match self.home {
            Some(ref path) => Ok(path.clone()),
            None => Ok(BaseDirs::new()
                .ok_or(Error::DirPath)?
                .home_dir()
                .join(APP_FOLDER)),
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

    pub fn get_jwt_secret_path(&self) -> Result<PathBuf, Error> {
        match self.eth.jwt_secret_path {
            Some(ref jwt_path) => Ok(jwt_path.clone()),
            None => Ok(PathBuf::from_str("./assets/jwtsecret").unwrap()),
        }
    }

    pub fn get_etherium_rpc(&self) -> Result<String, Error> {
        match self.eth.etherium_rpc_url {
            Some(ref url) => Ok(url.to_string()),
            None => Ok("http://localhost".to_string()),
        }
    }

    pub fn get_engine_rpc_url(&self) -> Result<String, Error> {
        match self.eth.engine_rpc_url {
            Some(ref url) => Ok(url.to_string()),
            None => Ok("http://localhost".to_string()),
        }
    }
}
