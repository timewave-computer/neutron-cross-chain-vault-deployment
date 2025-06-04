use serde::{Deserialize, Serialize};
use valence_strategist_utils::worker::ValenceWorkerTomlSerde;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GaiaStrategyConfig {
    pub grpc_url: String,
    pub grpc_port: String,
    pub chain_id: String,
    pub mnemonic: String,

    /// all denoms relevant to the gaia-side of strategy
    pub denoms: GaiaDenoms,
}

impl ValenceWorkerTomlSerde for GaiaStrategyConfig {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GaiaDenoms {
    /// e.g. WBTC
    pub deposit_token: String,
    /// gas fee denom
    pub atom: String,
}
