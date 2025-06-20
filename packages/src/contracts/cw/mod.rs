//! CosmWasm contract artifacts and utilities
//!
//! This module contains CosmWasm contract (.wasm) files and utilities
//! for the Cosmos ecosystem chains.

use std::include_bytes;

/// Valence Authorization contract bytecode
pub const VALENCE_AUTHORIZATION_BYTECODE: &[u8] = include_bytes!("valence_authorization.wasm");

/// Valence Base Account contract bytecode
pub const VALENCE_BASE_ACCOUNT_BYTECODE: &[u8] = include_bytes!("valence_base_account.wasm");

/// Valence Clearing Queue Supervaults contract bytecode
pub const VALENCE_CLEARING_QUEUE_SUPERVAULTS_BYTECODE: &[u8] =
    include_bytes!("valence_clearing_queue_supervaults.wasm");

/// Valence Dynamic Ratio Query Provider contract bytecode
pub const VALENCE_DYNAMIC_RATIO_QUERY_PROVIDER_BYTECODE: &[u8] =
    include_bytes!("valence_dynamic_ratio_query_provider.wasm");

/// Valence Forwarder Library contract bytecode
pub const VALENCE_FORWARDER_LIBRARY_BYTECODE: &[u8] =
    include_bytes!("valence_forwarder_library.wasm");

/// Valence ICA IBC Transfer contract bytecode
pub const VALENCE_ICA_IBC_TRANSFER_BYTECODE: &[u8] =
    include_bytes!("valence_ica_ibc_transfer.wasm");

/// Valence Interchain Account contract bytecode
pub const VALENCE_INTERCHAIN_ACCOUNT_BYTECODE: &[u8] =
    include_bytes!("valence_interchain_account.wasm");

/// Valence Mars Lending contract bytecode
pub const VALENCE_MARS_LENDING_BYTECODE: &[u8] = include_bytes!("valence_mars_lending.wasm");

/// Valence MaxBTC Issuer contract bytecode
pub const VALENCE_MAXBTC_ISSUER_BYTECODE: &[u8] = include_bytes!("valence_maxbtc_issuer.wasm");

/// Valence Processor contract bytecode
pub const VALENCE_PROCESSOR_BYTECODE: &[u8] = include_bytes!("valence_processor.wasm");

/// Valence Splitter Library contract bytecode
pub const VALENCE_SPLITTER_LIBRARY_BYTECODE: &[u8] =
    include_bytes!("valence_splitter_library.wasm");

/// Valence Supervaults LPer contract bytecode
pub const VALENCE_SUPERVAULTS_LPER_BYTECODE: &[u8] =
    include_bytes!("valence_supervaults_lper.wasm");

/// Valence Supervaults Withdrawer contract bytecode
pub const VALENCE_SUPERVAULTS_WITHDRAWER_BYTECODE: &[u8] =
    include_bytes!("valence_supervaults_withdrawer.wasm");

/// Valence Verification Gateway contract bytecode
pub const VALENCE_VERIFICATION_GATEWAY_BYTECODE: &[u8] =
    include_bytes!("valence_verification_gateway.wasm");

/// Get CosmWasm contract bytecode by name
pub fn get_contract_bytecode(name: &str) -> Option<&'static [u8]> {
    match name {
        "valence_authorization" => Some(VALENCE_AUTHORIZATION_BYTECODE),
        "valence_base_account" => Some(VALENCE_BASE_ACCOUNT_BYTECODE),
        "valence_clearing_queue_supervaults" => Some(VALENCE_CLEARING_QUEUE_SUPERVAULTS_BYTECODE),
        "valence_dynamic_ratio_query_provider" => {
            Some(VALENCE_DYNAMIC_RATIO_QUERY_PROVIDER_BYTECODE)
        }
        "valence_forwarder_library" => Some(VALENCE_FORWARDER_LIBRARY_BYTECODE),
        "valence_ica_ibc_transfer" => Some(VALENCE_ICA_IBC_TRANSFER_BYTECODE),
        "valence_interchain_account" => Some(VALENCE_INTERCHAIN_ACCOUNT_BYTECODE),
        "valence_mars_lending" => Some(VALENCE_MARS_LENDING_BYTECODE),
        "valence_maxbtc_issuer" => Some(VALENCE_MAXBTC_ISSUER_BYTECODE),
        "valence_processor" => Some(VALENCE_PROCESSOR_BYTECODE),
        "valence_splitter_library" => Some(VALENCE_SPLITTER_LIBRARY_BYTECODE),
        "valence_supervaults_lper" => Some(VALENCE_SUPERVAULTS_LPER_BYTECODE),
        "valence_supervaults_withdrawer" => Some(VALENCE_SUPERVAULTS_WITHDRAWER_BYTECODE),
        "valence_verification_gateway" => Some(VALENCE_VERIFICATION_GATEWAY_BYTECODE),
        _ => None,
    }
}

/// List all available CosmWasm contracts
pub fn list_contracts() -> Vec<&'static str> {
    vec![
        "valence_authorization",
        "valence_base_account",
        "valence_clearing_queue_supervaults",
        "valence_dynamic_ratio_query_provider",
        "valence_forwarder_library",
        "valence_ica_ibc_transfer",
        "valence_interchain_account",
        "valence_mars_lending",
        "valence_maxbtc_issuer",
        "valence_processor",
        "valence_splitter_library",
        "valence_supervaults_lper",
        "valence_supervaults_withdrawer",
        "valence_verification_gateway",
    ]
}

/// Contract metadata for deployment
#[derive(Debug, Clone)]
pub struct ContractMetadata {
    pub name: &'static str,
    pub description: &'static str,
    pub bytecode: &'static [u8],
}

/// Get contract metadata by name
pub fn get_contract_metadata(name: &str) -> Option<ContractMetadata> {
    match name {
        "valence_authorization" => Some(ContractMetadata {
            name: "valence_authorization",
            description: "Valence Authorization contract for access control",
            bytecode: VALENCE_AUTHORIZATION_BYTECODE,
        }),
        "valence_base_account" => Some(ContractMetadata {
            name: "valence_base_account",
            description: "Valence Base Account contract",
            bytecode: VALENCE_BASE_ACCOUNT_BYTECODE,
        }),
        "valence_clearing_queue_supervaults" => Some(ContractMetadata {
            name: "valence_clearing_queue_supervaults",
            description: "Valence Clearing Queue for Supervaults",
            bytecode: VALENCE_CLEARING_QUEUE_SUPERVAULTS_BYTECODE,
        }),
        "valence_dynamic_ratio_query_provider" => Some(ContractMetadata {
            name: "valence_dynamic_ratio_query_provider",
            description: "Valence Dynamic Ratio Query Provider",
            bytecode: VALENCE_DYNAMIC_RATIO_QUERY_PROVIDER_BYTECODE,
        }),
        "valence_forwarder_library" => Some(ContractMetadata {
            name: "valence_forwarder_library",
            description: "Valence Forwarder Library contract",
            bytecode: VALENCE_FORWARDER_LIBRARY_BYTECODE,
        }),
        "valence_ica_ibc_transfer" => Some(ContractMetadata {
            name: "valence_ica_ibc_transfer",
            description: "Valence ICA IBC Transfer contract",
            bytecode: VALENCE_ICA_IBC_TRANSFER_BYTECODE,
        }),
        "valence_interchain_account" => Some(ContractMetadata {
            name: "valence_interchain_account",
            description: "Valence Interchain Account contract",
            bytecode: VALENCE_INTERCHAIN_ACCOUNT_BYTECODE,
        }),
        "valence_mars_lending" => Some(ContractMetadata {
            name: "valence_mars_lending",
            description: "Valence Mars Protocol lending integration",
            bytecode: VALENCE_MARS_LENDING_BYTECODE,
        }),
        "valence_maxbtc_issuer" => Some(ContractMetadata {
            name: "valence_maxbtc_issuer",
            description: "Valence MaxBTC token issuer contract",
            bytecode: VALENCE_MAXBTC_ISSUER_BYTECODE,
        }),
        "valence_processor" => Some(ContractMetadata {
            name: "valence_processor",
            description: "Valence Processor contract for transaction processing",
            bytecode: VALENCE_PROCESSOR_BYTECODE,
        }),
        "valence_splitter_library" => Some(ContractMetadata {
            name: "valence_splitter_library",
            description: "Valence Splitter Library contract",
            bytecode: VALENCE_SPLITTER_LIBRARY_BYTECODE,
        }),
        "valence_supervaults_lper" => Some(ContractMetadata {
            name: "valence_supervaults_lper",
            description: "Valence Supervaults LP provider contract",
            bytecode: VALENCE_SUPERVAULTS_LPER_BYTECODE,
        }),
        "valence_supervaults_withdrawer" => Some(ContractMetadata {
            name: "valence_supervaults_withdrawer",
            description: "Valence Supervaults withdrawal contract",
            bytecode: VALENCE_SUPERVAULTS_WITHDRAWER_BYTECODE,
        }),
        "valence_verification_gateway" => Some(ContractMetadata {
            name: "valence_verification_gateway",
            description: "Valence Verification Gateway contract",
            bytecode: VALENCE_VERIFICATION_GATEWAY_BYTECODE,
        }),
        _ => None,
    }
}
