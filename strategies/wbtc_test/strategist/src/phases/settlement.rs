use std::error::Error;

use cosmwasm_std::to_json_binary;
use log::{info, warn};
use packages::{
    labels::{MARS_WITHDRAW_LABEL, SETTLE_OBLIGATION_LABEL},
    phases::SETTLEMENT_PHASE,
    utils::{obligation::flatten_obligation_queue_amounts, valence_core},
};
use valence_clearing_queue_supervaults::msg::ObligationsResponse;
use valence_domain_clients::cosmos::{base_client::BaseClient, wasm_client::WasmClient};

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

        // query the current settlement account balances
        let settlement_acc_bal_deposit = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
        let settlement_acc_bal_lp = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.supervault_lp,
            )
            .await?;
        info!(
            target: SETTLEMENT_PHASE,
            "settlement account deposit balance = {settlement_acc_bal_deposit}"
        );
        info!(target: SETTLEMENT_PHASE, "settlement account LP balance = {settlement_acc_bal_lp}");

        // query the Clearing Queue pending obligations
        let clearing_queue: ObligationsResponse = self
            .neutron_client
            .query_contract_state(
                &self.cfg.neutron.libraries.clearing_queue,
                valence_clearing_queue_supervaults::msg::QueryMsg::PendingObligations {
                    from: None,
                    to: None,
                },
            )
            .await?;
        info!(
            target: SETTLEMENT_PHASE, "clearing queue length = {}", clearing_queue.obligations.len()
        );

        // flatten the obligation response into amounts of relevant denoms
        let (deposit_obligation_total, lp_obligation_total) = flatten_obligation_queue_amounts(
            &clearing_queue.obligations,
            (
                self.cfg.neutron.denoms.deposit_token.to_string(),
                self.cfg.neutron.denoms.supervault_lp.to_string(),
            ),
        );

        info!(
            target: SETTLEMENT_PHASE, "total obligations deposit_token = {deposit_obligation_total}"
        );
        info!(
            target: SETTLEMENT_PHASE, "total obligations supervaults_lp = {lp_obligation_total}"
        );

        // 3. if settlement account balance is insufficient to cover the active
        // obligations, we perform the Mars protocol withdrawals
        if settlement_acc_bal_deposit < deposit_obligation_total {
            // 3. simulate Mars protocol withdrawal to obtain the funds necessary
            // to fulfill all active withdrawal requests
            let obligations_delta =
                deposit_obligation_total.saturating_sub(settlement_acc_bal_deposit);
            info!(
                target: SETTLEMENT_PHASE, "settlement_account deposit_token balance deficit = {obligations_delta}"
            );

            // 4. call the Mars lending library to perform the withdrawal.
            // This will deposit the underlying assets directly to the settlement account.
            info!(
                target: SETTLEMENT_PHASE, "withdrawing {obligations_delta} from mars lending position"
            );
            let mars_withdraw_msg =
                valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
                    valence_mars_lending::msg::FunctionMsgs::Withdraw {
                        amount: Some(obligations_delta.into()),
                    },
                );

            valence_core::enqueue_neutron(
                &self.neutron_client,
                &self.cfg.neutron.authorizations,
                MARS_WITHDRAW_LABEL,
                vec![to_json_binary(&mars_withdraw_msg)?],
            )
            .await?;

            valence_core::tick_neutron(&self.neutron_client, &self.cfg.neutron.processor).await?;
        }

        if settlement_acc_bal_lp < lp_obligation_total {
            warn!(target: SETTLEMENT_PHASE, "insufficient supervault LP share balance! available: {settlement_acc_bal_lp}, obligations: {lp_obligation_total}");
        }

        // 5. process the Clearing Queue settlement requests by enqueuing the settlement
        // messages to the processor and ticking
        for obligation in clearing_queue.obligations {
            info!(
                target: SETTLEMENT_PHASE, "settling obligation #{}", obligation.id
            );

            // build the settlement function message
            let settlement_exec_msg =
                valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
                    valence_clearing_queue_supervaults::msg::FunctionMsgs::SettleNextObligation {},
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
