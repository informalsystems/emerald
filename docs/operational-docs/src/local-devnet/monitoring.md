# Monitoring

The `make` command automatically starts monitoring services to help you observe network behavior.

## Grafana - Metrics Visualization

**URL**: http://localhost:4000

Grafana provides visual dashboards for monitoring validator and network metrics.

**Default credentials:**
- Username: `admin`
- Password: `admin` (you'll be prompted to change this on first login, but you can skip it for local testing)

**What to monitor:**
- **Block production rate**: Are validators producing blocks consistently?
- **Consensus metrics**: Round times, vote counts, proposal statistics
- **Node health**: CPU, memory, disk usage
- **Network metrics**: Peer connections, message rates

**Tip**: If you don't see data immediately, wait 30-60 seconds for metrics to accumulate.

## Prometheus - Raw Metrics

**URL**: http://localhost:9090

Prometheus collects time-series metrics from all nodes. Use the query interface to explore raw metrics data.

**Useful queries:**
- `emerald_consensus_height` - Current consensus height per node
- `emerald_consensus_round` - Current consensus round
- `emerald_mempool_size` - Number of transactions in mempool
- `process_cpu_seconds_total` - CPU usage per process

**When to use Prometheus:**
- Creating custom queries
- Debugging specific metric issues
- Exporting data for analysis

## Otterscan - Block Explorer

**URL**: http://localhost:5100

Otterscan is a lightweight block explorer for inspecting blocks, transactions, and accounts.

**Features:**
- View recent blocks and transactions
- Search by address, transaction hash, or block number
- Inspect contract interactions
- View account balances and transaction history

**Use cases:**
- Verify transactions were included in blocks
- Debug smart contract interactions
- Inspect validator activity
- View network state

## Emerald Node Logs

View consensus logs for each validator:

```bash
# View logs from validator 0
tail -f nodes/0/emerald.log

# View logs from all validators simultaneously
tail -f nodes/{0,1,2,3}/emerald.log
```

**What to look for:**
- Block proposals and commits
- Consensus round progression
- Validator voting activity
- Any errors or warnings

## Docker Container Logs

View Reth execution client logs:

```bash
# View logs from Reth node 0
docker compose logs -f reth0

# View all Reth logs
docker compose logs -f reth0 reth1 reth2 reth3
```

**What to look for:**
- Block execution confirmations
- Transaction processing
- Peer connection status
- Engine API communication with Emerald