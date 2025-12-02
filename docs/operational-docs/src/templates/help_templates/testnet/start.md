Generate testnet configuration (explicit) Start a complete testnet with Reth + Emerald nodes

Usage: emerald testnet start [OPTIONS]

Options:
      --home <HOME_DIR>
          Home directory for Malachite (default: `$HOME/.emerald-devnet`)
  -n, --nodes <NODES>
          Number of node pairs to create (max 20) [default: 3]
      --log-level <LOG_LEVEL>
          Log level (default: `malachite=debug`)
      --node-keys <NODE_KEYS>
          Private keys for validators (can be specified multiple times) Supports both hex format (0x...) and JSON format from init command
      --emerald-bin <EMERALD_BIN>
          Path to the `emerald` executable. The program first checks the path provided here; if the binary is not found, it will try to resolve `emerald` from $PATH instead [default: ./target/debug/emerald]
      --log-format <LOG_FORMAT>
          Log format (default: `plaintext`)
      --config <CONFIG_FILE>
          Emerald configuration file (default: `~/.emerald/config/config.toml`)
      --emerald-utils-bin <EMERALD_UTILS_BIN>
          Path to the `emerald-utils` executable. The program first checks the path provided here; if the binary is not found, it will try to resolve `emerald-utils` from $PATH instead [default: ./target/debug/emerald-utils]
      --custom-reth-bin <CUSTOM_RETH_BIN>
          Path to the `custom-reth` executable. The program first checks the path provided here; if the binary is not found, it will try to resolve `custom-reth` from $PATH instead [default: ./custom-reth/target/debug/custom-reth]
      --reth-config-path <RETH_CONFIG_PATH>
          Path to reth node spawning configurations. If not specified will use default values
  -h, --help
          Print help