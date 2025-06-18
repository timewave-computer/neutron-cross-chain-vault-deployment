use std::error::Error;

use cosmwasm_std::to_json_binary;
use log::{info, warn};
use types::labels::{MARS_WITHDRAW_LABEL, SETTLE_OBLIGATION_LABEL};
use valence_clearing_queue_supervaults::msg::ObligationsResponse;
use valence_domain_clients::cosmos::{base_client::BaseClient, wasm_client::WasmClient};

use crate::strategy_config::Strategy;

const SETTLEMENT_PHASE: &str = "settlement";

impl Strategy {
    /// performs the final settlement of registered withdrawal obligations in
    /// the Clearing Queue library. this involves topping up the settlement
    /// account with funds necessary to carry out all withdrawal obligations
    /// in the queue.
    pub async fn settlement(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!(target: SETTLEMENT_PHASE, "starting settlement phase");

        // 1. query the current settlement account balance
        let settlement_acc_bal_deposit_token_bal = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
        let settlement_acc_bal_supervaults = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.supervault_lp,
            )
            .await?;
        info!(
            target: SETTLEMENT_PHASE,
            "settlement account balance deposit_token = {settlement_acc_bal_deposit_token_bal}"
        );
        info!(
            target: SETTLEMENT_PHASE,
            "settlement account balance supervaults_lp = {settlement_acc_bal_supervaults}"
        );

        // 2. query the Clearing Queue and calculate the total active obligations
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

        let mut deposit_obligation_total = 0;
        let mut lp_obligation_total = 0;

        // iterate through all obligations and sum up the coin amounts
        for withdraw_obligation in clearing_queue.obligations.iter() {
            for payout_coin in withdraw_obligation.payout_coins.iter() {
                if payout_coin.denom == self.cfg.neutron.denoms.deposit_token {
                    deposit_obligation_total += payout_coin.amount.u128();
                } else if payout_coin.denom == self.cfg.neutron.denoms.supervault_lp {
                    lp_obligation_total += payout_coin.amount.u128();
                } else {
                    warn!(target: SETTLEMENT_PHASE, "obligation contains unrecognized denom: {}", payout_coin.denom);
                }
            }
        }

        info!(
            target: SETTLEMENT_PHASE, "total obligations deposit_token = {deposit_obligation_total}"
        );
        info!(
            target: SETTLEMENT_PHASE, "total obligations supervaults_lp = {lp_obligation_total}"
        );

        // 3. if settlement account balance is insufficient to cover the active
        // obligations, we perform the Mars protocol withdrawals
        if settlement_acc_bal_deposit_token_bal < deposit_obligation_total {
            // 3. simulate Mars protocol withdrawal to obtain the funds necessary
            // to fulfill all active withdrawal requests
            let obligations_delta = deposit_obligation_total - settlement_acc_bal_deposit_token_bal;
            info!(
                target: SETTLEMENT_PHASE, "settlement_account deposit_token balance deficit = {obligations_delta}"
            );

            // 4. call the Mars lending library to perform the withdrawal.
            // This will deposit the underlying assets directly to the settlement account.
            info!(
                target: SETTLEMENT_PHASE, "withdrawing {obligations_delta} from mars lending position"
            );
            let mars_withdraw_msg =
                to_json_binary(&valence_mars_lending::msg::FunctionMsgs::Withdraw {
                    amount: Some(obligations_delta.into()),
                })?;

            self.enqueue_neutron(MARS_WITHDRAW_LABEL, vec![mars_withdraw_msg])
                .await?;

            self.tick_neutron().await?;
        }

        // 5. process the Clearing Queue settlement requests by enqueuing the settlement
        // messages to the processor and ticking
        for obligation in clearing_queue.obligations {
            info!(
                target: SETTLEMENT_PHASE, "settling obligation #{}", obligation.id
            );
            let obligation_settlement_msg = to_json_binary(
                &valence_clearing_queue_supervaults::msg::FunctionMsgs::SettleNextObligation {},
            )?;

            self.enqueue_neutron(SETTLE_OBLIGATION_LABEL, vec![obligation_settlement_msg])
                .await?;

            self.tick_neutron().await?;
        }

        Ok(())
    }
}
