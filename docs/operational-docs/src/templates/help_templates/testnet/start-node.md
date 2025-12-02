Restart an existing stopped node by ID

Usage: emerald testnet start-node [OPTIONS] <NODE_ID>

Arguments:
  <NODE_ID>  Node ID to start

Options:
      --emerald-bin <EMERALD_BIN>
          Path to the `emerald` executable. The program first checks the path provided here; if the binary is not found, it will try to resolve `emerald` from $PATH instead [default: ./target/debug/emerald]
      --home <HOME_DIR>
          Home directory for Malachite (default: `$HOME/.emerald-devnet`)
      --custom-reth-bin <CUSTOM_RETH_BIN>
          Path to the `custom-reth` executable. The program first checks the path provided here; if the binary is not found, it will try to resolve `custom-reth` from $PATH instead [default: ./custom-reth/target/debug/custom-reth]
      --log-level <LOG_LEVEL>
          Log level (default: `malachite=debug`)
      --log-format <LOG_FORMAT>
          Log format (default: `plaintext`)
      --reth-config-path <RETH_CONFIG_PATH>
          Path to reth node spawning configurations. If not specified will use default values
      --config <CONFIG_FILE>
          Emerald configuration file (default: `~/.emerald/config/config.toml`)
  -h, --help
          Print help