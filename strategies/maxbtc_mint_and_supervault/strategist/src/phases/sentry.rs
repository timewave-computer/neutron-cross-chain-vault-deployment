use std::time::Duration;

use log::info;
use packages::phases::SENTRY_PHASE;
use tokio::time::sleep;

use crate::strategy_config::Strategy;

impl Strategy {
    /// basic sentry phase which sleeps for the duration configured
    /// in the strategist config.
    /// for more elaborate sentry configurations, see the strategist
    /// getting started guide.
    pub async fn sentry(&mut self) -> anyhow::Result<()> {
        info!(target: SENTRY_PHASE, "sleeping for {}sec", self.timeout);
        sleep(Duration::from_secs(self.timeout)).await;

        Ok(())
    }
}
