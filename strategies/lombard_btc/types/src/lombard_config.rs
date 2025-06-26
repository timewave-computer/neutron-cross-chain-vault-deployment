use serde::{Deserialize, Serialize};
use valence_strategist_utils::worker::ValenceWorkerTomlSerde;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LombardStrategyConfig {
    /// grpc node url
    pub grpc_url: String,
    /// grpc node port
    pub grpc_port: String,
    /// lombard chain id
    pub chain_id: String,

    /// lbtc denom on lombard
    pub deposit_denom: String,
    /// recovery ica address
    pub ica: String,

    /// ibc eureka denom that arrives from eth
    pub eureka_denom: String,
    /// native denom issued on lombard chain
    pub native_denom: String,

    /// IBC entry point contract addr
    pub entry_contract: String,
    /// IBC callback contract addr
    pub callback_contract: String,
}

impl ValenceWorkerTomlSerde for LombardStrategyConfig {}
