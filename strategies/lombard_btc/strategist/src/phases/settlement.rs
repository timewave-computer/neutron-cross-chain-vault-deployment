use cosmwasm_std::to_json_binary;
use log::{info, warn};
use packages::{
    labels::{MARS_WITHDRAW_LABEL, SETTLE_OBLIGATION_LABEL},
    phases::SETTLEMENT_PHASE,
    utils::{obligation::batch_obligation_queue_payouts, valence_core},
};
use valence_clearing_queue_supervaults::{msg::ObligationsResponse, state::WithdrawalObligation};
use valence_domain_clients::cosmos::{base_client::BaseClient, wasm_client::WasmClient};

use crate::strategy_config::Strategy;

impl Strategy {
    /// performs the final settlement of registered withdrawal obligations in
    /// the Clearing Queue library. this involves topping up the settlement
    /// account with funds necessary to clear out all withdrawal obligations
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
    pub async fn settlement(&mut self) -> anyhow::Result<()> {
        info!(target: SETTLEMENT_PHASE, "starting settlement phase");

        // query the current settlement account balances
        let settlement_bal_deposit = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
        let settlement_bal_lp = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.supervault_lp,
            )
            .await?;
        info!(target: SETTLEMENT_PHASE, "settlement deposit balance = {settlement_bal_deposit}");
        info!(target: SETTLEMENT_PHASE, "settlement LP balance = {settlement_bal_lp}");

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

        // batch all active clearing queue obligations
        let batched_obligation_coins = batch_obligation_queue_payouts(&obligations);

        for obligation_coin in batched_obligation_coins {
            if obligation_coin.denom == self.cfg.neutron.denoms.deposit_token {
                info!(target: SETTLEMENT_PHASE, "batched deposit_token obligation = {obligation_coin}");
                // if there is insufficient amount of deposit token denom in the settlement account,
                // we need to withdraw the delta from Mars lending position
                if settlement_bal_deposit < obligation_coin.amount.u128() {
                    self.handle_deposit_denom_obligation(obligation_coin, settlement_bal_deposit)
                        .await?;
                }
            } else if obligation_coin.denom == self.cfg.neutron.denoms.supervault_lp {
                info!(target: SETTLEMENT_PHASE, "batched supervaults_lp obligation = {obligation_coin}");
                if settlement_bal_lp < obligation_coin.amount.u128() {
                    warn!(target: SETTLEMENT_PHASE, "insufficient supervault LP share balance!
                        available: {settlement_bal_lp}, obligation: {obligation_coin}");
                }
            } else {
                warn!(target: SETTLEMENT_PHASE, "unexpected coin among obligations: {obligation_coin}");
            }
        }

        // process the Clearing Queue settlement requests by enqueuing the settlement
        // messages to the processor and ticking
        self.clear_withdraw_obligations(obligations).await?;

        Ok(())
    }

    /// pulls out the necessary amount of liquidity from a Mars lending position
    /// in order to facilitate the obligation clearing
    async fn handle_deposit_denom_obligation(
        &mut self,
        obligation_coin: cosmwasm_std::Coin,
        settlement_acc_deposit_bal: u128,
    ) -> anyhow::Result<()> {
        // find the amount to be withdrawn
        let obligations_delta = obligation_coin
            .amount
            .u128()
            .saturating_sub(settlement_acc_deposit_bal);

        info!(
            target: SETTLEMENT_PHASE, "settlement_account deposit_token balance deficit = {obligations_delta}"
        );
        info!(
            target: SETTLEMENT_PHASE, "withdrawing {obligations_delta} from mars lending position"
        );

        // call the Mars lending library to perform the withdrawal.
        // This will deposit the underlying assets directly to the settlement account.
        let mars_withdraw_msg = valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
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

        Ok(())
    }

    /// attempts to settle a vector of `WithdrawalObligation`s by enqueuing an
    /// obligation settlement request and ticking the processor for each element
    /// in the vector
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
