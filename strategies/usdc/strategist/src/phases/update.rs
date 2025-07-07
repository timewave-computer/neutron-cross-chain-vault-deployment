use log::info;
use packages::phases::UPDATE_PHASE;
use valence_domain_clients::evm::base_client::CustomProvider;

use crate::strategy_config::Strategy;

impl Strategy {
    pub async fn update(&mut self, eth_rp: &CustomProvider) -> anyhow::Result<()> {
        info!(target: UPDATE_PHASE, "starting vault update phase");

        Ok(())
    }
}
