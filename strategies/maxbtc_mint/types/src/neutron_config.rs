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

    /// contract where maxBTCs are minted
    pub maxbtc_contract: String,

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
    /// maxBTC denom
    pub maxbtc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutronAccounts {
    /// Valence ICA
    pub gaia_ica: String,
    /// deposit account where funds will arrive from cosmos hub
    pub deposit: String,
    /// settlement account where funds will be sent to end users
    pub settlement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutronLibraries {
    /// MaxBTC issuer library that will be used to mint maxBTC by depositing funds from deposit account
    pub maxbtc_issuer: String,
    /// Clearing queue
    pub clearing_queue: String,
    /// ICA transfer libraries
    pub ica_transfer_gaia: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutronCoprocessorAppIds {
    /// Clearing queue
    pub clearing_queue: String,
}
