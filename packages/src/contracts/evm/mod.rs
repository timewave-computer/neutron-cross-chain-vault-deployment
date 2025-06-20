//! EVM contract artifacts and utilities
//!
//! This module contains Solidity contract files and their compiled artifacts
//! for the Ethereum Virtual Machine (EVM) compatible chains.

use std::include_bytes;

/// Authorization contract bytecode
pub const AUTHORIZATION_BYTECODE: &[u8] = include_bytes!("Authorization.sol/Authorization.json");

/// BaseAccount contract bytecode
pub const BASE_ACCOUNT_BYTECODE: &[u8] = include_bytes!("BaseAccount.sol/BaseAccount.json");

/// ERC1967Proxy contract bytecode
pub const ERC1967_PROXY_BYTECODE: &[u8] = include_bytes!("ERC1967Proxy.sol/ERC1967Proxy.json");

/// ERC20 contract bytecode
pub const ERC20_BYTECODE: &[u8] = include_bytes!("ERC20.sol/ERC20.json");

/// IBCEurekaTransfer contract bytecode
pub const IBC_EUREKA_TRANSFER_BYTECODE: &[u8] =
    include_bytes!("IBCEurekaTransfer.sol/IBCEurekaTransfer.json");

/// LiteProcessor contract bytecode
pub const LITE_PROCESSOR_BYTECODE: &[u8] = include_bytes!("LiteProcessor.sol/LiteProcessor.json");

/// OneWayVault contract bytecode
pub const ONE_WAY_VAULT_BYTECODE: &[u8] = include_bytes!("OneWayVault.sol/OneWayVault.json");

/// SP1VerificationGateway contract bytecode
pub const SP1_VERIFICATION_GATEWAY_BYTECODE: &[u8] =
    include_bytes!("SP1VerificationGateway.sol/SP1VerificationGateway.json");

/// Get contract bytecode by name
pub fn get_contract_bytecode(name: &str) -> Option<&'static [u8]> {
    match name {
        "Authorization" => Some(AUTHORIZATION_BYTECODE),
        "BaseAccount" => Some(BASE_ACCOUNT_BYTECODE),
        "ERC1967Proxy" => Some(ERC1967_PROXY_BYTECODE),
        "ERC20" => Some(ERC20_BYTECODE),
        "IBCEurekaTransfer" => Some(IBC_EUREKA_TRANSFER_BYTECODE),
        "LiteProcessor" => Some(LITE_PROCESSOR_BYTECODE),
        "OneWayVault" => Some(ONE_WAY_VAULT_BYTECODE),
        "SP1VerificationGateway" => Some(SP1_VERIFICATION_GATEWAY_BYTECODE),
        _ => None,
    }
}

/// List all available EVM contracts
pub fn list_contracts() -> Vec<&'static str> {
    vec![
        "Authorization",
        "BaseAccount",
        "ERC1967Proxy",
        "ERC20",
        "IBCEurekaTransfer",
        "LiteProcessor",
        "OneWayVault",
        "SP1VerificationGateway",
    ]
}
