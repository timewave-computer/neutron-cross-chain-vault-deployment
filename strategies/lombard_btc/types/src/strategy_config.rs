use serde::{Deserialize, Serialize};

use crate::{
    ethereum_config::EthereumStrategyConfig, gaia_config::GaiaStrategyConfig,
    neutron_config::NeutronStrategyConfig,
};

/// top-level config that wraps around each domain configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub ethereum: EthereumStrategyConfig,
    pub neutron: NeutronStrategyConfig,
    pub gaia: GaiaStrategyConfig,
}
