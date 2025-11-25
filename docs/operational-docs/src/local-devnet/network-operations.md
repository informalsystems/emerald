# Network Operations

## Stop the Network

```bash
make stop
```

This stops all Docker containers but preserves data.

## Clean the Network

```bash
make clean
```

**Warning**: This deletes:

- All node data (`nodes/`)
- Genesis file (`assets/genesis.json`)
- Testnet config (`.testnet/`)
- Docker volumes (Reth databases)
- Prometheus/Grafana data

## Restart a Clean Network 

```bash
make clean
make
```