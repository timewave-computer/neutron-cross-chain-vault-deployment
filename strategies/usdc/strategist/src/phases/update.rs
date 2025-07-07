use valence_domain_clients::evm::base_client::CustomProvider;

use crate::strategy_config::Strategy;

impl Strategy {
    pub async fn update(&mut self, eth_rp: &CustomProvider) -> anyhow::Result<()> {
        Ok(())
    }
}
