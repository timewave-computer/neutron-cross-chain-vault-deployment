use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthereumStrategyConfig {
    /// ethereum node rpc url
    pub rpc_url: String,
    /// strategist mnemonic
    pub mnemonic: String,

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
