use alloy::primitives::{Address, U256};
use cosmwasm_std::Uint128;
use serde::{Deserialize, Serialize};
use valence_strategist_utils::worker::ValenceWorkerTomlSerde;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthereumStrategyConfig {
    /// ethereum node rpc url
    pub rpc_url: String,

    /// rate update thresholds. if newly calculated rate
    /// would result in an increase or decrease relative
    /// to the current rate that would exceed these values,
    /// vault gets paused and the update is skipped.
    pub max_rate_increment_bps: u64,
    pub max_rate_decrement_bps: u64,

    /// minimum eureka-transfer input account balance
    /// to perform IBC transfer (taking fees into account)
    pub ibc_transfer_threshold_amt: U256,

    /// update rate scaling factor
    pub rate_scaling_factor: Uint128,

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
