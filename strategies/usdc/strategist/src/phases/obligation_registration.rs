use log::info;
use packages::phases::REGISTRATION_PHASE;

use crate::strategy_config::Strategy;

impl Strategy {
    pub async fn register_withdraw_obligations(&mut self) -> anyhow::Result<()> {
        info!(target: REGISTRATION_PHASE, "starting withdraw obligation registration phase");

        Ok(())
    }
}
