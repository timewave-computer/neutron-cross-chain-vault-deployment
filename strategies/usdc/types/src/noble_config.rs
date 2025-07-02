use serde::{Deserialize, Serialize};
use valence_strategist_utils::worker::ValenceWorkerTomlSerde;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NobleStrategyConfig {
    pub grpc_url: String,
    pub grpc_port: String,
    pub chain_id: String,
    // native chain denom
    pub chain_denom: String,
    pub uusdc_denom: String,
    // ICA address
    pub ica_address: String,
}

impl ValenceWorkerTomlSerde for NobleStrategyConfig {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NobleDenoms {
    pub usdc: String,
}
