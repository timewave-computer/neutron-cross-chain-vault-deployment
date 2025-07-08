use cosmwasm_std::Uint128;
use serde::{Deserialize, Serialize};
use valence_strategist_utils::worker::ValenceWorkerTomlSerde;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutronStrategyConfig {
    /// grpc node url
    pub grpc_url: String,
    /// grpc node port
    pub grpc_port: String,
    /// neutron chain id
    pub chain_id: String,
    /// total amount of untrn required to initiate an ibc transfer from neutron
    pub min_ibc_fee: Uint128,

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
    /// All IDs of the coprocessor apps
    pub coprocessor_app_ids: NeutronCoprocessorAppIds,
}

impl ValenceWorkerTomlSerde for NeutronStrategyConfig {}

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
    /// settlement account where funds will be sent to end users
    pub settlement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutronLibraries {
    /// Supervault lper
    pub supervault_lper: String,
    /// Clearing queue
    pub clearing_queue: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutronCoprocessorAppIds {
    /// Clearing queue
    pub clearing_queue: String,
}
