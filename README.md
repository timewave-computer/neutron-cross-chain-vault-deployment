# Neutron Cross-Chain Vault Deployment

A production-ready cross-chain Bitcoin vault system built on the Valence Protocol that enables users to deposit WBTC on Ethereum while generating yield on Neutron through Mars Protocol lending and Supervaults.

## Architecture

The system operates across three blockchain networks:
- **Ethereum**: User-facing ERC-4626 vault where users deposit collateral
- **Neutron**: CosmWasm-based liquidity provision via Mars Protocol + Supervaults  
- **Cosmos Hub**: IBC/ICA bridging/messaging between Ethereum and Neutron
- **IBC Eureka**: Ethereum ↔ Cosmos Hub bridging

## Components

- **`strategist/`**: Automated off-chain solver orchestrating cross-chain operations
- **`deploy/`**: Deployment automation for Neutron (CosmWasm) and Ethereum (Solidity) contracts
- **`types/`**: Shared type definitions and configuration management

## How It Works

1. Users deposit collateral tokens into Ethereum ERC-4626 vault, receive vault shares
2. Strategist monitors deposits and bridges funds: Ethereum → Cosmos Hub → Neutron
3. In Phase 1 funds deployed to Mars Protocol
4. In Phase 2 funds are deployed to the Neutron Supervaults
5. Withdrawal requests use ZK proofs for cross-chain state verification
6. The Strategist orchestrates the cross-chain program and updates the redemption rate on Ethereum

## Quick Start

```bash
# Build the project
cargo build --release

# Deploy contracts (requires configuration)
cargo run -p deploy --bin main

# Run strategist (requires .env configuration)
cargo run -p strategist --bin main
```

## Documentation

- [Strategist Getting Started Guide](strategist/strategist_getting_started.md)
- [Deployment Guide](deploy/README.md)
