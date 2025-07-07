use alloy::providers::Provider;
use log::info;
use packages::{phases::UPDATE_PHASE, types::sol_types::OneWayVault};
use valence_domain_clients::evm::base_client::{CustomProvider, EvmBaseClient};

use crate::strategy_config::Strategy;

impl Strategy {
    pub async fn update(&mut self, eth_rp: &CustomProvider) -> anyhow::Result<()> {
        info!(target: UPDATE_PHASE, "starting vault update phase");

        let one_way_vault_contract =
            OneWayVault::new(self.cfg.ethereum.libraries.one_way_vault, &eth_rp);

        let current_vault_rate = self
            .eth_client
            .query(one_way_vault_contract.redemptionRate())
            .await?
            ._0;

        info!(target: UPDATE_PHASE, "current vault redemption rate: {current_vault_rate}");

        info!(target: UPDATE_PHASE, "updating ethereum vault redemption rate");
        let update_request = one_way_vault_contract
            .update(current_vault_rate)
            .into_transaction_request();

        let update_vault_exec_response = self.eth_client.sign_and_send(update_request).await?;

        eth_rp
            .get_transaction_receipt(update_vault_exec_response.transaction_hash)
            .await?;

        Ok(())
    }
}
