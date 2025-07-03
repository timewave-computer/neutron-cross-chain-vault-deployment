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

    /// Mars protocol credit manager
    pub mars_credit_manager: String,
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
    /// Valence ICA
    pub ica: String,
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
    /// Deposit splitter where funds will be moved from deposit account to both Mars deposit or Supervault deposit according to the split ratio
    pub deposit_splitter: String,
    /// Mars lending library
    pub mars_lending: String,
    /// Supervault lper
    pub supervault_lper: String,
    /// Clearing queue
    pub clearing_queue: String,
    /// ICA transfer library
    pub ica_transfer: String,
    /// phase shift maxBTC issuer library
    pub phase_shift_maxbtc_issuer: String,
    /// phase shift forwarder
    pub phase_shift_forwarder: String,
    /// phase shift supervault withdrawer
    pub phase_shift_supervault_withdrawer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutronCoprocessorAppIds {
    /// Clearing queue
    pub clearing_queue: String,
}
