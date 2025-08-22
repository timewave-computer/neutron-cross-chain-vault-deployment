use cosmwasm_std::to_json_binary;
use log::info;
use packages::{labels::SETTLE_OBLIGATION_LABEL, phases::SETTLEMENT_PHASE, utils::valence_core};
use valence_clearing_queue_supervaults::{msg::ObligationsResponse, state::WithdrawalObligation};
use valence_domain_clients::cosmos::{base_client::BaseClient, wasm_client::WasmClient};

use crate::strategy_config::Strategy;

impl Strategy {
    pub async fn settlement(&mut self) -> anyhow::Result<()> {
        info!(target: SETTLEMENT_PHASE, "starting settlement phase");

        let settlement_bal_maxbtc = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.maxbtc,
            )
            .await?;
        info!(target: SETTLEMENT_PHASE, "settlement maxBTC balance = {settlement_bal_maxbtc}");

        // query the Clearing Queue pending obligations
        let ObligationsResponse { obligations } = self
            .neutron_client
            .query_contract_state(
                &self.cfg.neutron.libraries.clearing_queue,
                valence_clearing_queue_supervaults::msg::QueryMsg::PendingObligations {
                    from: None,
                    to: None,
                },
            )
            .await?;

        // early return if there is nothing to settle
        if obligations.is_empty() {
            info!(target: SETTLEMENT_PHASE, "no obligations to settle; concluding settlement phase");
            return Ok(());
        }

        for o in &obligations {
            info!(target: SETTLEMENT_PHASE, "obligation #{} payouts: {:?}", o.id, o.payout_coins);
        }

        // process the Clearing Queue settlement requests by enqueuing the settlement
        // messages to the processor and ticking
        self.clear_withdraw_obligations(obligations).await?;

        Ok(())
    }

    async fn clear_withdraw_obligations(
        &mut self,
        obligations: Vec<WithdrawalObligation>,
    ) -> anyhow::Result<()> {
        let settlement_exec_msg = valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
            valence_clearing_queue_supervaults::msg::FunctionMsgs::SettleNextObligation {},
        );

        for obligation in obligations {
            info!(
                target: SETTLEMENT_PHASE, "settling obligation #{}", obligation.id
            );

            // enqueue the settlement message and tick the processor
            valence_core::enqueue_neutron(
                &self.neutron_client,
                &self.cfg.neutron.authorizations,
                SETTLE_OBLIGATION_LABEL,
                vec![to_json_binary(&settlement_exec_msg)?],
            )
            .await?;

            valence_core::tick_neutron(&self.neutron_client, &self.cfg.neutron.processor).await?;
        }

        Ok(())
    }
}
