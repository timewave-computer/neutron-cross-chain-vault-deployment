use crate::strategy_config::Strategy;

impl Strategy {
    pub async fn settlement(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}
