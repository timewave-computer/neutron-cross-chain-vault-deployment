use std::error::Error;

use log::trace;
use packages::{
    phases::UPDATE_PHASE,
    types::sol_types::{BaseAccount, ERC20, OneWayVault},
};
use valence_domain_clients::evm::base_client::CustomProvider;

use crate::strategy_config::Strategy;

impl Strategy {
    /// performs the vault rate update
    pub async fn update(
        &mut self,
        eth_rp: &CustomProvider,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        trace!(target: UPDATE_PHASE, "starting vault update phase");

        let eth_deposit_acc_contract =
            BaseAccount::new(self.cfg.ethereum.accounts.deposit, &eth_rp);
        let one_way_vault_contract =
            OneWayVault::new(self.cfg.ethereum.libraries.one_way_vault, &eth_rp);
        let eth_deposit_denom_contract =
            ERC20::new(self.cfg.ethereum.denoms.deposit_token, &eth_rp);

        Ok(())
    }
}
