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

    /// Mars protocol wbtc contract
    pub mars_pool: String,
    /// Supervaults vault addresses
    pub fbtc_supervault: String,
    pub lbtc_supervault: String,
    pub solvbtc_supervault: String,
    pub ebtc_supervault: String,
    pub pumpbtc_supervault: String,
    pub bedrockbtc_supervault: String,
    pub maxbtc_supervault: String,
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
    /// supervaults LP share denoms (These need to be updated post migration)
    pub fbtc_supervault_lp: String,
    pub lbtc_supervault_lp: String,
    pub solvbtc_supervault_lp: String,
    pub ebtc_supervault_lp: String,
    pub pumpbtc_supervault_lp: String,
    pub bedrockbtc_supervault_lp: String,
    pub maxbtc_supervault_lp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutronAccounts {
    /// Valence ICA
    pub ica: String,
    /// deposit account where funds will arrive from cosmos hub
    pub deposit: String,
    /// input account from which funds will be deposited into Mars
    pub mars_deposit: String,
    /// input accounts for all supervaults
    pub fbtc_supervault_deposit: String,
    pub lbtc_supervault_deposit: String,
    pub solvbtc_supervault_deposit: String,
    pub ebtc_supervault_deposit: String,
    pub pumpbtc_supervault_deposit: String,
    pub bedrockbtc_supervault_deposit: String,
    pub maxbtc_supervault_deposit: String,
    /// settlement account where funds will be sent to end users
    pub settlement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutronLibraries {
    /// Deposit splitter where funds will be moved from deposit account to both Mars deposit or Supervault deposit according to the split ratio
    pub deposit_splitter: String,
    /// Mars lending library
    pub mars_lending: String,
    /// Supervault lpers
    pub fbtc_supervault_lper: String,
    pub lbtc_supervault_lper: String,
    pub solvbtc_supervault_lper: String,
    pub ebtc_supervault_lper: String,
    pub pumpbtc_supervault_lper: String,
    pub bedrockbtc_supervault_lper: String,
    pub maxbtc_supervault_lper: String,
    /// Clearing queue
    pub clearing_queue: String,
    /// ICA transfer library
    pub ica_transfer: String,
    /// phase shift maxBTC issuer library
    pub phase_shift_maxbtc_issuer: String,
    /// phase shift splitter
    pub phase_shift_splitter: String,
    /// phase shift forwarder
    pub phase_shift_forwarder: String,
    /// phase shift supervault withdrawers
    pub phase_shift_fbtc_supervault_withdrawer: String,
    pub phase_shift_lbtc_supervault_withdrawer: String,
    pub phase_shift_solvbtc_supervault_withdrawer: String,
    pub phase_shift_ebtc_supervault_withdrawer: String,
    pub phase_shift_pumpbtc_supervault_withdrawer: String,
    pub phase_shift_bedrockbtc_supervault_withdrawer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutronCoprocessorAppIds {
    /// Clearing queue
    pub clearing_queue: String,
}
