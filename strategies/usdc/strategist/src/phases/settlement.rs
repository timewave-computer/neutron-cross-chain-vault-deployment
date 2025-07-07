use log::info;
use packages::phases::SETTLEMENT_PHASE;

use crate::strategy_config::Strategy;

impl Strategy {
    pub async fn settlement(&mut self) -> anyhow::Result<()> {
        info!(target: SETTLEMENT_PHASE, "starting settlement phase");

        Ok(())
    }
}
