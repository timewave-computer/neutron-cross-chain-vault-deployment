use serde::{Deserialize, Serialize};
use valence_strategist_utils::worker::ValenceWorkerTomlSerde;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoprocessorStrategyConfig {
    /// coprocessor circuit IDs
    pub eureka_circuit_id: String,
    pub vault_circuit_id: String,
}

impl ValenceWorkerTomlSerde for CoprocessorStrategyConfig {}
