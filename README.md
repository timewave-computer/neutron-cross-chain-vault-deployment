# Neutron Cross-Chain Vault Deployment

A production-ready cross-chain vault system built on the Valence Protocol that enables users to deposit selected denoms on Ethereum while generating yield on Neutron through Mars Protocol lending and Supervaults.

## Architecture

The system operates across three blockchain networks:
- **Ethereum**: User-facing ERC-4626 vault where users deposit collateral
- **Neutron**: CosmWasm-based liquidity provision via Mars Protocol, Supervaults, and MaxBTC strategy vaults
- **Cosmos Hub**: IBC/ICA bridging/messaging between Ethereum and Neutron
- **IBC Eureka**: Ethereum ↔ Cosmos Hub bridging
- **ZK Co-processor**: Generating proofs for IBC Eureka route queries and withdrawal obligations

## Components

- **`packages/`**: Common utility types and functions used across different vaults
- **`strategies/`**: Directory for storing different strategies, each of which contain:
  - **`deploy/`**: Deployment automation for Neutron (CosmWasm) and Ethereum (Solidity) contracts
  - **`strategist/`**: Automated off-chain solver orchestrating cross-chain operations
  - **`types/`**: Shared type definitions and configuration management

## How It Works

1. Users deposit collateral tokens into Ethereum ERC-4626 vault, receive vault shares
2. Strategist monitors user deposits and bridges the funds: Ethereum → Cosmos Hub/Noble → Neutron
   The IBC Eureka route fetched from the Skip API is validated by the co-processor
3. Strategist deploys bridged assets into Mars Protocol and Neutron Supervaults
4. Users issue withdraw requests into Ethereum ERC-4626 vault, immediately burning their shares
5. Strategist picks up user withdraw requests from the indexer and posts them to the co-processor
   for validation
6. Strategist posts co-processor withdraw request proofs to the Neutron Authorizations contract
   which turns them into withdrawal obligations that get enqueued into the Clearing Queue contract
7. Strategist withdraws the funds necessary to cover the outstanding withdraw obligations and clears them
8. The Strategist calculates the new redemption rate and posts it to the ERC-4626 vault, concluding
   the cycle

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
