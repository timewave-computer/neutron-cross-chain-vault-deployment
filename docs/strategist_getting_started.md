# Strategist Getting Started Guide

This guide covers how to configure, run, monitor, and recover the Neutron Cross-Chain Vault Strategist.

## Overview

The Strategist is an automated off-chain solver that orchestrates cross-chain operations for the Neutron Cross-Chain Vault. It implements the `ValenceWorker` trait and performs periodic cycles of operations including deposit processing, withdrawal registration, settlement, and vault updates.

## Dependencies

The strategist requires access to:
- **Ethereum RPC**: For monitoring vault events and executing transactions
- **Neutron gRPC**: For CosmWasm contract interactions
- **Cosmos Hub gRPC**: For IBC and Interchain Account operations
- **Valence Coprocessor RPC**: For generating zero-knowledge proofs for cross-chain state verification
- **Indexer API**: For efficient event querying and transaction tracking
- **Lombard gRPC**: (only applicable for LBTC vault) For Lombard recovery address accounting

## Configuration

#### Environment Variables

Create a `.env` file in the project root with the following required variables:

```bash
# Required environment variables
MNEMONIC="your 24-word mnemonic phrase here"
LABEL="strategist-production"  # or "strategist-testnet", etc.
INDEXER_API_KEY="neutron_team_api_key"
INDEXER_API_URL="https://indexer.valence.zone"

# Optional logging configuration
RUST_LOG="info,strategist=debug"
```

#### Configuration Files

The strategist requires four TOML configuration files:

**1. Ethereum Configuration (`ethereum_config.toml`)**

```toml
rpc_url = "https://mainnet.infura.io/v3/YOUR_API_KEY"
ibc_transfer_threshold_amt = "1000000"  # Minimum amount for transfers (in wei)

# Contract addresses (replace with actual deployed addresses)
authorizations = "0x..."
processor = "0x..."

[denoms]
deposit_token = "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599"  # WBTC

[accounts]
deposit = "0x..."

[libraries]
one_way_vault = "0x..."
eureka_transfer = "0x..."
```

**2. Neutron Configuration (`neutron_config.toml`)**

```toml
grpc_url = "https://rpc.neutron.quokkastake.io"
grpc_port = "9090"
chain_id = "neutron-1"
min_ibc_fee = "1000"

# Protocol addresses
mars_pool = "neutron1..."
supervault = "neutron1..."

# Contract addresses
authorizations = "neutron1..."
processor = "neutron1..."

[denoms]
deposit_token = "ibc/D742E8566B0B8CC8F569D950051C09CF57988A88F0E45574BFB3079D41DE6462"
ntrn = "untrn"
supervault_lp = "factory/..."

[accounts]
deposit = "neutron1..."
mars_deposit = "neutron1..."
supervault_deposit = "neutron1..."
settlement = "neutron1..."

[libraries]
deposit_forwarder = "neutron1..."
mars_lending = "neutron1..."
supervault_lper = "neutron1..."
clearing_queue = "neutron1..."
```

**3. Cosmos Hub Configuration (`gaia_config.toml`)**

```toml
grpc_url = "https://cosmos-rpc.polkachu.com"
grpc_port = "9090"
chain_id = "cosmoshub-4"
chain_denom = "uatom"
btc_denom = "ibc/D742E8566B0B8CC8F569D950051C09CF57988A88F0E45574BFB3079D41DE6462"

# Note: mnemonic is read from MNEMONIC environment variable, not stored in config
```

### Valence Coprocessor Configuration (`coprocessor_config.toml`)

```toml
# Replace with actual Valence Coprocessor endpoint when available
rpc_url = "https://api.valence.so/coprocessor"  # or internal endpoint
rpc_port = "443"
api_version = "v1"

# ZK proof generation settings
proof_timeout_seconds = 300
max_retries = 3
batch_size = 10

# Optional authentication (if required)
# api_key = "your_api_key_here"  # Read from environment if needed

[proof_types]
ethereum_state_proof = "ethereum_state_v1"
withdrawal_verification = "withdrawal_v1"

# Note: The actual coprocessor endpoint may be internal/private
# Contact Valence team for the correct production endpoint
```

## Building and Running

### Build Commands

```bash
# Build in development mode
cargo build

# Build optimized release version
cargo build --release

# Build only the strategist
cargo build -p strategist --release
```

## Starting the Strategist

### Manual Start

```bash
# Start with default configuration
cargo run --bin main --release

# Start with custom configuration files
CONFIG_DIR=/path/to/configs cargo run --bin main --release

# Start with increased logging
RUST_LOG=debug cargo run --bin main --release
```

### Production Start (using systemd)

Create a systemd service file `/etc/systemd/system/neutron-strategist.service`:

```ini
[Unit]
Description=Neutron Cross-Chain Vault Strategist
After=network.target

[Service]
Type=simple
User=strategist
WorkingDirectory=/opt/neutron-strategist
Environment=RUST_LOG=info
Environment=LABEL=strategist-production
EnvironmentFile=/etc/strategist/environment
ExecStart=/opt/neutron-strategist/target/release/main
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

Create the environment file `/etc/strategist/environment` containing your mnemonic:

```bash
MNEMONIC=your_24_word_mnemonic_phrase_here
```

Enable and start the service:

```bash
sudo systemctl daemon-reload
sudo systemctl enable neutron-strategist
sudo systemctl start neutron-strategist
```

## Stopping the Strategist

#### Manual Stop

```bash
# Stop the strategist
pkill -f "strategist"
```

#### Systemd Stop

```bash
# Stop the service
sudo systemctl stop neutron-strategist

# Stop and disable
sudo systemctl stop neutron-strategist
sudo systemctl disable neutron-strategist
```

## Restarting the Strategist

#### Manual Restart

```bash
# Restart
pkill -f "strategist"
cargo run --bin main --release

```

#### Systemd Restart

```bash
# Restart service
sudo systemctl restart neutron-strategist

# Reload configuration and restart
sudo systemctl reload-or-restart neutron-strategist
```

## Monitoring and Logging

#### Log Configuration

The strategist uses the `log` crate with the following levels:

```bash
# Environment variable options
export RUST_LOG="info"                    # Basic logging
export RUST_LOG="debug"                   # Detailed logging
export RUST_LOG="strategist=debug,info"   # Debug strategist, info others
export RUST_LOG="trace"                   # Maximum verbosity
```

#### Log Locations

**Development:**
Logs output to console when running with `cargo run`

**Production (systemd):**
```bash
# View logs
sudo journalctl -u neutron-strategist -f

# View recent logs
sudo journalctl -u neutron-strategist --since "1 hour ago"

# View logs with specific priority
sudo journalctl -u neutron-strategist -p err
```

#### Local Strategist Process Monitoring

All logs can be found at `/var/log/strategist.log`

It's a good idea to keep track of these log messages:

1. **Cycle Execution**: Successful completion of deposit, withdrawal, and settlement cycles
2. **RPC Connectivity**: Connection status to all three chains and Coprocessor
3. **Transaction Success**: Success rate of submitted transactions
4. **Error Rates**: Frequency and types of errors encountered


#### On-Chain Monitoring

**Valence Program Explorer**: [https://app.valence.zone/programs](https://app.valence.zone/programs)
- View account balances for program accounts
- Monitor on-chain contract state and configurations
- Debug Neutron-side transactions and contract interactions
- Track program execution history and state changes

*Note: Currently Neutron only*

## Operational Commands

#### Status Checks

```bash
# Check if strategist is running
ps aux | grep strategist

# Check systemd status
sudo systemctl status neutron-strategist
```

#### Configuration Updates

```bash
# 1. Stop strategist
sudo systemctl stop neutron-strategist

# 2. Update configuration files
vim /etc/strategist/neutron_config.toml

# 3. Validate configuration (if validation tool exists)
cargo run --bin validate-config

# 4. Restart strategist
sudo systemctl start neutron-strategist
```

#### Read-only Mode

- Set `ibc_transfer_threshold_amt` to a very high value
- This prevents actual fund transfers while maintaining monitoring

#### Recovery

1. Stop strategist
2. Check chain synchronization status
3. Verify account balances
4. Review recent transaction history
5. Restart with appropriate configuration

## Health Checks
```bash
# Check Neutron RPC connectivity
curl -s https://rpc.neutron.quokkastake.io/status

# Check Neutron gRPC connectivity
curl -s https://rpc.neutron.quokkastake.io:9090 || echo "gRPC port not reachable via HTTP"

# Check Cosmos Hub RPC connectivity
curl -s https://cosmos-rpc.polkachu.com/status

# Check Cosmos Hub gRPC connectivity
curl -s https://cosmos-rpc.polkachu.com:9090 || echo "Cosmos Hub gRPC port not reachable via HTTP"

# Check Ethereum latest block (via Timewave prover)
curl -s http://prover.timewave.computer:37281/api/registry/domain/ethereum-alpha/latest | jq '.number'

# Check coprocessor status
curl -s http://prover.timewave.computer:37281/api/stats

# Check indexer API connectivity
curl -s "https://indexer.valence.zone/health" --header "X-Api-Key: ${INDEXER_API_KEY}"

# Example indexer query for vault withdraw requests
curl -s "https://indexer.valence.zone/v1/vault/0xf2B85C389A771035a9Bd147D4BF87987A7F9cf98/withdrawRequests?from=0" \
  --header "X-Api-Key: ${INDEXER_API_KEY}"
```
