use std::time::Duration;

use log::info;
use packages::{phases::SENTRY_PHASE, utils::valence_core};
use tokio::time::sleep;

use crate::strategy_config::Strategy;

impl Strategy {
    /// basic sentry phase which sleeps for the duration configured
    /// in the strategist config.
    /// for more elaborate sentry configurations, see the strategist
    /// getting started guide.
    pub async fn sentry(&mut self) -> anyhow::Result<()> {
        // before starting the cycle we flush any existing items
        // from the processor queue
        valence_core::flush_neutron_processor_queue(
            &self.neutron_client,
            &self.cfg.neutron.processor,
            valence_authorization_utils::authorization::Priority::Medium,
        )
        .await?;

        info!(target: SENTRY_PHASE, "sleeping for {}sec", self.timeout);
        sleep(Duration::from_secs(self.timeout)).await;

        Ok(())
    }
}
