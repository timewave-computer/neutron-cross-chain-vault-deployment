use std::error::Error;

use log::info;
use packages::phases::SETTLEMENT_PHASE;

use crate::strategy_config::Strategy;

impl Strategy {
    /// performs the final settlement of registered withdrawal obligations in
    /// the Clearing Queue library. this involves topping up the settlement
    /// account with funds necessary to carry out all withdrawal obligations
    /// in the queue.
    /// consists of the following stages:
    /// 1. query the pending obligations clearing queue and batch them up
    /// 2. ensure the queue is ready to be cleared:
    ///   1. if settlement account deposit token balance is insufficient
    ///      to clear the entire queue, withdraw the necessary amount from Mars
    ///   2. if settlement account LP token balance is insufficient to clear
    ///      the entire queue, log a warning message (this should not happen
    ///      with correct configuration)
    /// 3. clear the queue in a FIFO manner
    pub async fn settlement(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!(target: SETTLEMENT_PHASE, "starting settlement phase");

        Ok(())
    }
}
