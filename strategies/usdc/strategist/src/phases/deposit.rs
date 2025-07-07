use log::info;
use packages::phases::DEPOSIT_PHASE;
use valence_domain_clients::evm::base_client::CustomProvider;

use crate::strategy_config::Strategy;

impl Strategy {
    pub async fn deposit(&mut self, eth_rp: &CustomProvider) -> anyhow::Result<()> {
        info!(target: DEPOSIT_PHASE, "starting deposit phase");

        Ok(())
    }
}
