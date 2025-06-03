use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GaiaStrategyConfig {
    pub grpc_url: String,
    pub grpc_port: String,
    pub chain_id: String,
    pub mnemonic: String,
    // native chain denom
    pub chain_denom: String,

    // deposit denom routed from ethereum via IBC-Eureka
    pub btc_denom: String,
}
