use alloy::primitives::U256;
use serde::{Deserialize, Serialize};
use valence_strategist_utils::worker::ValenceWorkerTomlSerde;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthereumStrategyConfig {
    /// ethereum node rpc url
    pub rpc_url: String,

    /// minimum eureka-transfer input account balance
    /// to perform IBC transfer (taking fees into account)
    pub ibc_transfer_threshold_amt: U256,

    /// authorizations module
    pub authorizations: String,
    /// lite-processor coupled with the authorizations
    pub processor: String,

    /// all denoms relevant to the eth-side of strategy
    pub denoms: EthereumDenoms,
    /// all accounts relevant to the eth-side of strategy
    pub accounts: EthereumAccounts,
    /// all libraries relevant to the eth-side of strategy
    pub libraries: EthereumLibraries,
}

impl ValenceWorkerTomlSerde for EthereumStrategyConfig {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthereumDenoms {
    /// e.g. WBTC ERC20 address
    pub deposit_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthereumAccounts {
    /// deposit account where user deposits will settle
    /// until being IBC-Eureka'd out to Cosmos Hub
    pub deposit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthereumLibraries {
    /// ERC-4626-based vault
    pub one_way_vault: String,
    /// IBC-Eureka transfer
    pub eureka_transfer: String,
}
