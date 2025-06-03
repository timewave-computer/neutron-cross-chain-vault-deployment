use cosmwasm_std::Uint128;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutronStrategyConfig {
    /// grpc node url
    pub grpc_url: String,
    /// grpc node port
    pub grpc_port: String,
    /// neutron chain id
    pub chain_id: String,
    /// strategist mnemonic
    pub mnemonic: String,
    /// total amount of untrn required to initiate an ibc transfer from neutron
    pub min_ibc_fee: Uint128,

    /// Mars protocol wbtc contract
    pub mars_pool: String,
    /// Supervaults vault address
    pub supervault: String,

    /// authorizations module
    pub authorizations: String,
    /// processor coupled with the authorizations
    pub processor: String,

    /// all denoms relevant to the neutron-side of strategy
    pub denoms: NeutronDenoms,
    /// all accounts relevant to the neutron-side of strategy
    pub accounts: NeutronAccounts,
    /// all libraries relevant to the neutron-side of strategy
    pub libraries: NeutronLibraries,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutronDenoms {
    /// e.g. WBTC
    pub deposit_token: String,
    /// gas fee denom
    pub ntrn: String,
    /// supervaults LP share denom
    pub supervault_lp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutronAccounts {
    /// deposit account where funds will arrive from cosmos hub
    pub deposit: String,
    /// input account from which funds will be deposited into Mars
    pub mars_deposit: String,
    /// input account from which funds will be deposited into Supervault
    pub supervault_deposit: String,
    /// settlement account where funds will be sent to end users
    pub settlement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutronLibraries {
    /// Deposit forwarder where funds will be moved from deposit account to either Mars deposit or Supervault deposit
    pub deposit_forwarder: String,
    /// Mars lending library
    pub mars_lending: String,
    /// Supervault lper
    pub supervault_lper: String,
    /// Clearing queue
    pub clearing_queue: String,
}
