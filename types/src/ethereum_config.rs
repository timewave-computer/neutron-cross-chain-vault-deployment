use alloy::primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use valence_strategist_utils::worker::ValenceWorkerTomlSerde;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthereumStrategyConfig {
    /// ethereum node rpc url
    pub rpc_url: String,

    /// minimum eureka-transfer input account balance
    /// to perform IBC transfer (taking fees into account)
    pub ibc_transfer_threshold_amt: U256,

    /// update rate scaling factor
    pub rate_scaling_factor: u128,

    /// authorizations module
    pub authorizations: Address,
    /// lite-processor coupled with the authorizations
    pub processor: Address,

    /// all denoms relevant to the eth-side of strategy
    pub denoms: EthereumDenoms,
    /// all accounts relevant to the eth-side of strategy
    pub accounts: EthereumAccounts,
    /// all libraries relevant to the eth-side of strategy
    pub libraries: EthereumLibraries,
    /// all coprocessor app ids relevant to the eth-side of strategy
    pub coprocessor_app_ids: EthereumCoprocessorAppIds,
}

impl ValenceWorkerTomlSerde for EthereumStrategyConfig {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthereumDenoms {
    /// e.g. WBTC ERC20 address
    pub deposit_token: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthereumAccounts {
    /// deposit account where user deposits will settle
    /// until being IBC-Eureka'd out to Cosmos Hub
    pub deposit: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthereumLibraries {
    /// ERC-4626-based vault
    pub one_way_vault: Address,
    /// IBC-Eureka transfer
    pub eureka_transfer: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthereumCoprocessorAppIds {
    /// IBC Eureka
    pub ibc_eureka: String,
}
