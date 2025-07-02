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

## Roles and Permissions

| Role | Description | Responsible Entity | Network |
|------|-------------|------------|---------|
| Strategist | Orchestrates cross-chain operations with highly constrained actions limited to pre-defined routes. Critical responsibility is to update the redemption rate regularly. Failure to update the rate automatically pauses the vault, requiring the owner to unpause | Hadron Labs | Both Neutron and Ethereum |
| Vault Owner | Controls the vault contract parameters and emergency functions. Only the owner can upgrade and pause the contracts | [Neutron DAO (multisig)](https://app.safe.global/home?safe=eth:0x54a37ac81263C482D6BE56F5Bd796e06e9Afa344) on Ethereum, [Neutron Valence-specific security DAO](https://daodao.zone/dao/neutron1h2lzp88kjk24sf7jfyrpd27xzfp52qerwvyxx2ds23pwavhz72asrpacva/home) on Neutron | Both Neutron and Ethereum |
| Verification Gateway Owner | Manages the verification gateway for cross-chain state verification | Valency Security Committee | Both Neutron and Ethereum |

The co-processor and light client services are trustless services managed by Valence.

## Documentation

- [Strategist Getting Started Guide](./docs/strategist_getting_started.md)
- [Deployment Guide](./docs/deploy_getting_started.md)
