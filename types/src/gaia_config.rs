use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GaiaStrategyConfig {
    pub grpc_url: String,
    pub grpc_port: String,
    pub chain_id: String,
    pub mnemonic: String,
    pub denom: String,
}
