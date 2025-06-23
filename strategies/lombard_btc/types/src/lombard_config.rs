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
}

impl ValenceWorkerTomlSerde for LombardStrategyConfig {}
