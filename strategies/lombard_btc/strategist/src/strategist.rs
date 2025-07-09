use std::{error::Error, time::Duration};

use async_trait::async_trait;
use log::info;
use packages::phases::VALENCE_WORKER;
use tokio::time::sleep;
use valence_domain_clients::evm::{
    base_client::CustomProvider, request_provider_client::RequestProviderClient,
};
use valence_strategist_utils::worker::ValenceWorker;

use crate::strategy_config::Strategy;

// implement the ValenceWorker trait for the Strategy struct.
// This trait defines the main loop of the strategy and inherits
// the default implementation for spawning the worker.
#[async_trait]
impl ValenceWorker for Strategy {
    fn get_name(&self) -> String {
        format!("Valence X-Vault: {}", self.label)
    }

    async fn cycle(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!(target: VALENCE_WORKER, "sleeping for {}sec", self.timeout);
        sleep(Duration::from_secs(self.timeout)).await;

        info!(target: VALENCE_WORKER, "{}: Starting cycle...", self.get_name());

        let eth_rp: CustomProvider = self.eth_client.get_request_provider().await?;

        // first we carry out the deposit flow
        self.deposit(&eth_rp).await?;

        // after deposit flow is complete, we process the new obligations
        self.register_withdraw_obligations().await?;

        // with new obligations registered into the clearing queue, we
        // carry out the settlements
        self.settlement().await?;

        // having processed all new exit requests after the deposit flow,
        // the epoch is ready to be concluded.
        // we perform the final accounting flow and post vault update.
        // self.update(&eth_rp).await?;

        Ok(())
    }
}
