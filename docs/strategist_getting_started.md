# Strategist Getting Started Guide

## Overview

The Strategist is an automated off-chain solver that orchestrates the cross-chain operations for the Neutron Cross-Chain Vault. It implements the `ValenceWorker` trait, performing periodic cycles of operations including deposit processing, withdrawal registration and settlement, and vault rate updates. Its primary responsibility is to monitor on-chain events and execute the necessary transactions to manage the vault's liquidity and maintain its health.

The strategist is designed to be operationally stateless, meaning it does not rely on a local database to function. Instead, it uses the state of the underlying Valence programs as its single source of truth.

Strategist operations are idempotent; if the strategist is stopped at any point during its runtime and resumed afterwards, it will not lead to any duplicate transaction execution. Instead, it will run its cycle, eventually reaching the point at which it was previously stopped and continuing from thereon.

## Dependencies

The strategist requires network access to the following services:

- **Ethereum RPC**: For monitoring vault events and executing transactions on Ethereum.
- **Neutron gRPC**: For all CosmWasm contract interactions on Neutron.
- **Cosmos Hub gRPC**: For IBC and Interchain Account (ICA) operations on the Cosmos Hub.
- **Valence Coprocessor**: For generating zero-knowledge proofs for cross-chain state verification.
- **Valence Indexer API**: For efficiently querying on-chain events and vault state.
- **IBC Eureka API**: For finding cross-chain routes from Ethereum.
- **Lombard gRPC**: (For `lombard_btc` strategy only) For interacting with the Lombard protocol.
- **Noble gRPC**: (For `usdc` strategy only) For interacting with the Noble protocol.
- **OTLP API**: For posting the execution logs to a Grafana dashboard for easier debugging.

## Configuration

The strategist is configured through a combination of environment variables and TOML files.

### Configuration Files

The strategist requires four `.toml` files that contain the on-chain addresses and parameters generated during the [deployment process](./deploy_getting_started.md).

-   **`neutron_strategy_config.toml`**: Contains all contract addresses, account addresses, asset denoms, and relevant co-processor IDs for the Neutron network.
-   **`ethereum_strategy_config.toml`**: Contains all contract addresses, account addresses, asset denoms, and relevant co-processor IDs for the Ethereum network.
-   **`gaia_strategy_config.toml`**: Contains the gRPC endpoints and chain parameters for the Cosmos Hub, including the ICA address.
-   **`lombard_strategy_config.toml`**: (For `lombard_btc` strategy only) Contains the gRPC endpoints and chain parameters for the Lombard protocol.
-   **`noble_strategy_config.toml`**: (For `usdc` strategy only) Contains the gRPC endpoints and chain parameters for the Noble protocol.

### Environment Variables

For Strategist operations, an environment file needs to be made available at the desired strategy `strategist` directory. For lombard_btc strategy, this can be done as follows:

```bash
cd strategies/lombard_btc/strategist
cp lbtc.env.example lbtc.env
```

After that, modify the env file values:

-   `ETHEREUM_CFG_PATH`: Path to the `ethereum_strategy_config.toml`.
-   `GAIA_CFG_PATH`: Path to the `gaia_strategy_config.toml`.
-   `LOMBARD_CFG_PATH`: Path to the `lombard_strategy_config.toml`.
-   `MNEMONIC`: The 24-word mnemonic phrase for the strategist's wallet.
-   `LABEL`: A unique identifier for the strategist instance (e.g., "X_LBTC_PROD").
-   `STRATEGY_TIMEOUT`: The delay in seconds between each operational cycle.
-   `INDEXER_API_KEY`: Your API key for the Valence Indexer.
-   `INDEXER_API_URL`: The endpoint for the Valence Indexer.
-   `EUREKA_API_URL`: The endpoint for the IBC Eureka API.
-   `OTLP_ENDPOINT`: (optional) The endpoint for an OpenTelemetry collector to send structured logs.

## Running the Strategist

Once your environment is configured, you can start the strategist from the project's root directory:

```bash
# Replace <strategy-name> with 'lombard_btc', 'wbtc', etc.
just start <strategy-name>
```

## How It Works

The strategist operates in a continuous cycle, executing a series of phases to manage the vault's funds and state. The `strategist.rs` file defines the main cycle, which calls the different phases in a specific order.

### The Strategist Cycle

1.  **Deposit:** Manages new user deposit flows.
2.  **Register Withdraw Obligations:** Processes new withdrawal requests, generates their ZKPs, and posts them to the Neutron Authorizations contract.
3.  **Settlement:** Settles the withdrawal obligations.
4.  **Update:** Calculates and updates the vault's redemption rate.

## Monitoring and Operations

### Logging

**OpenTelemetry Logging (`OTLP`)**: If you provide an `OTLP_ENDPOINT` environment variable, the strategist will send structured, machine-readable logs to that endpoint. This is highly recommended for production environments to integrate with monitoring and alerting platforms (e.g., DataDog, Grafana, etc.). The service name is hardcoded to `neutron-strategist`.

### Recovery

If the strategist crashes or enters a bad state, follow these general steps to recover:

1.  **Stop the Process**: Ensure any running instance of the strategist is stopped to prevent further actions.
2.  **Analyze Logs**: Review the logs (both console and OpenTelemetry, if configured) to identify the root cause of the failure.
3.  **Check Dependencies**: Verify that all external services (RPC nodes, APIs) are online and reachable.
4.  **Verify On-Chain State**: Check block explorers for the relevant chains to understand the state of the contracts. Verify account balances and look for any stuck or pending transactions.
5.  **Restart**: Once the issue has been identified and resolved (e.g., a dependency is back online, a configuration has been fixed), restart the strategist. It is designed to be stateless (other than the domain configs) and should be able to pick up where it left off.
