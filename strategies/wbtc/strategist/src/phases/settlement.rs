use std::{collections::HashMap, error::Error};

use cosmwasm_std::to_json_binary;
use log::{info, warn};
use packages::{
    labels::{MARS_WITHDRAW_LABEL, SETTLE_OBLIGATION_LABEL},
    phases::SETTLEMENT_PHASE,
    utils::{obligation::batch_obligation_queue_payouts, valence_core},
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

        let mut settlement_acc_balances = HashMap::new();

        // build up the settlement account balance map
        {
            let deposit_token_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.settlement,
                    &self.cfg.neutron.denoms.deposit_token,
                )
                .await?;
            settlement_acc_balances
                .insert(&self.cfg.neutron.denoms.deposit_token, deposit_token_bal);

            let lbtc_lp_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.settlement,
                    &self.cfg.neutron.denoms.lbtc_supervault_lp,
                )
                .await?;
            settlement_acc_balances
                .insert(&self.cfg.neutron.denoms.lbtc_supervault_lp, lbtc_lp_bal);

            let bedrockbtc_lp_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.settlement,
                    &self.cfg.neutron.denoms.bedrockbtc_supervault_lp,
                )
                .await?;
            settlement_acc_balances.insert(
                &self.cfg.neutron.denoms.bedrockbtc_supervault_lp,
                bedrockbtc_lp_bal,
            );

            let ebtc_lp_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.settlement,
                    &self.cfg.neutron.denoms.ebtc_supervault_lp,
                )
                .await?;
            settlement_acc_balances
                .insert(&self.cfg.neutron.denoms.ebtc_supervault_lp, ebtc_lp_bal);

            let fbtc_lp_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.settlement,
                    &self.cfg.neutron.denoms.fbtc_supervault_lp,
                )
                .await?;
            settlement_acc_balances
                .insert(&self.cfg.neutron.denoms.fbtc_supervault_lp, fbtc_lp_bal);

            let pumpbtc_lp_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.settlement,
                    &self.cfg.neutron.denoms.pumpbtc_supervault_lp,
                )
                .await?;
            settlement_acc_balances.insert(
                &self.cfg.neutron.denoms.pumpbtc_supervault_lp,
                pumpbtc_lp_bal,
            );

            let solvbtc_lp_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.settlement,
                    &self.cfg.neutron.denoms.solvbtc_supervault_lp,
                )
                .await?;
            settlement_acc_balances.insert(
                &self.cfg.neutron.denoms.solvbtc_supervault_lp,
                solvbtc_lp_bal,
            );
        }

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

        let obligation_amounts_map = batch_obligation_queue_payouts(&clearing_queue.obligations);

        info!(
            target: SETTLEMENT_PHASE, "total obligations = {obligation_amounts_map:?}"
        );

        // iterate over the batched obligations
        for (obligation_denom, obligation_amount) in obligation_amounts_map {
            // if settlement acc doesn't have the denom, default to 0
            let settlement_acc_obligation_denom_bal = settlement_acc_balances
                .get(&obligation_denom)
                .cloned()
                .unwrap_or_default();

            // if current iteration denom is the deposit token, we may need to withdraw
            // the diff from mars lending to be able to fulfill the obligations
            if obligation_denom == self.cfg.neutron.denoms.deposit_token {
                let obligations_delta = obligation_amount - settlement_acc_obligation_denom_bal;
                info!(
                    target: SETTLEMENT_PHASE, "settlement_account deposit_token balance deficit = {obligations_delta}"
                );

                // call the Mars lending library to perform the withdrawal.
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

                valence_core::tick_neutron(&self.neutron_client, &self.cfg.neutron.processor)
                    .await?;
            } else if settlement_acc_obligation_denom_bal < obligation_amount {
                // if settlement account balance is insufficient, something likely went wrong
                // with the configuration. this will require manual intervention, so we do not error
                // out in case previously observed obligations were valid and could be settled.
                warn!(target: SETTLEMENT_PHASE, "insufficient {obligation_denom} balance for settlement!
                    available: {settlement_acc_obligation_denom_bal}, obligation amt: {obligation_amount}");
            }
        }

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
