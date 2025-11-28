# Configuration Examples

This section contains example configuration files that you can use as templates for setting up your Emerald network.

## Emerald Configuration

- **[emerald-config.toml](config-examples/emerald-config.toml)** - Main configuration file for the Emerald consensus client
  - Contains execution client connection settings
  - JWT authentication configuration
  - Node identification and networking settings

## MalachiteBFT Configuration

- **[malachitebft-config.toml](config-examples/malachitebft-config.toml)** - Configuration for the underlying Malachite BFT consensus engine
  - Consensus timing and parameters
  - P2P networking configuration
  - Peer connection settings
  - Metrics endpoints

## Systemd Service Files

These example systemd service files can be used to run Emerald and Reth as system services on Linux servers.

- **[emerald.systemd.service.example](config-examples/emerald.systemd.service.example)** - Systemd service configuration for Emerald consensus client
- **[reth.systemd.server.example](config-examples/reth.systemd.server.example)** - Systemd service configuration for Reth execution client

## Usage

To use these configuration files:

1. Copy the relevant file to your configuration directory
2. Modify the values to match your network setup
3. Ensure file permissions are appropriate (especially for files containing sensitive data)
4. For systemd services, copy to `/etc/systemd/system/` and enable with `systemctl enable <service-name>`

## Important Notes

- Never commit configuration files with real private keys or secrets to version control
- Always use environment-specific values for production deployments
- Review and understand all configuration options before deploying
- Keep backup copies of your configuration files in a secure location
