use std::cmp::Ordering;

use alloy::{primitives::U256, providers::Provider};
use anyhow::anyhow;
use cosmwasm_std::{Decimal, Uint128};
use log::{info, warn};
use packages::{
    phases::UPDATE_PHASE,
    types::sol_types::{BaseAccount, ERC20, OneWayVault},
    utils::supervaults::Supervaults,
};
use valence_domain_clients::{
    cosmos::base_client::BaseClient,
    evm::base_client::{CustomProvider, EvmBaseClient},
};

use crate::strategy_config::Strategy;

impl Strategy {
    pub async fn update(&mut self, eth_rp: &CustomProvider) -> anyhow::Result<()> {
        info!(target: UPDATE_PHASE, "starting vault update phase");

        let one_way_vault_contract =
            OneWayVault::new(self.cfg.ethereum.libraries.one_way_vault, &eth_rp);

        let total_assets = self.total_deposit_assets(eth_rp).await?;
        info!(target: UPDATE_PHASE, "total deposit-token assets: {total_assets}");

        // fetch the total issued shares and convert them to u128
        let total_shares = self.total_issued_shares(eth_rp).await?;
        info!(target: UPDATE_PHASE, "eth_vault_issued_shares_u128={total_shares}");

        // rate =  effective_total_assets / (effective_vault_shares * scaling_factor)
        // multiplying the denominator by the scaling factor
        let scaled_shares_amount =
            Uint128::from(total_shares).checked_mul(self.cfg.ethereum.rate_scaling_factor)?;
        let redemption_rate_decimal =
            Decimal::checked_from_ratio(total_assets, scaled_shares_amount)?;
        info!(target: UPDATE_PHASE, "redemption rate decimal={redemption_rate_decimal}");

        let redemption_rate_sol_u256 = U256::try_from(redemption_rate_decimal.atomics().u128())?;
        info!(target: UPDATE_PHASE, "redemption_rate_sol_u256={redemption_rate_sol_u256}");

        // validate that the newly calculated redemption rate does not exceed
        // the max rate update thresholds relative to the current rate
        self.validate_new_redemption_rate(eth_rp, redemption_rate_sol_u256)
            .await?;

        info!(target: UPDATE_PHASE, "updating ethereum vault redemption rate to {redemption_rate_sol_u256}");
        let update_request = one_way_vault_contract
            .update(redemption_rate_sol_u256)
            .into_transaction_request();

        let update_vault_exec_response = self.eth_client.sign_and_send(update_request).await?;

        eth_rp
            .get_transaction_receipt(update_vault_exec_response.transaction_hash)
            .await?;

        Ok(())
    }

    async fn validate_new_redemption_rate(
        &self,
        eth_rp: &CustomProvider,
        new_redemption_rate: U256,
    ) -> anyhow::Result<()> {
        let one_way_vault_contract =
            OneWayVault::new(self.cfg.ethereum.libraries.one_way_vault, &eth_rp);

        let current_vault_rate = self
            .eth_client
            .query(one_way_vault_contract.redemptionRate())
            .await?
            ._0;

        let current_rate_u128 = u128::try_from(current_vault_rate)?;
        info!(target: UPDATE_PHASE, "pre_update_rate = {current_rate_u128}");

        // get the ratio of newly calculated redemption rate over the previous rate
        let redemption_rate_u128 = u128::try_from(new_redemption_rate)?;
        info!(target: UPDATE_PHASE, "new_rate = {redemption_rate_u128}");

        let rate_change_decimal =
            Decimal::checked_from_ratio(redemption_rate_u128, current_rate_u128)?;

        info!(target: UPDATE_PHASE, "new to old rate ratio = {rate_change_decimal}");

        match rate_change_decimal.cmp(&Decimal::one()) {
            // rate change is less than 1.0 -> redemption rate decreased
            Ordering::Less => {
                let rate_delta = Decimal::one() - rate_change_decimal;
                info!(target: UPDATE_PHASE, "redemption rate epoch delta = -{rate_delta}");

                let decrement_threshold = Decimal::bps(self.cfg.ethereum.max_rate_decrement_bps);
                if rate_delta > decrement_threshold {
                    warn!(target: UPDATE_PHASE, "rate delta exceeds the threshold of {decrement_threshold}; pausing the vault");
                    let pause_request = one_way_vault_contract.pause().into_transaction_request();
                    let pause_vault_exec_response =
                        self.eth_client.sign_and_send(pause_request).await?;
                    eth_rp
                        .get_transaction_receipt(pause_vault_exec_response.transaction_hash)
                        .await?;

                    return Err(anyhow!(
                        "newly calculated rate exceeds the rate update thresholds"
                    ));
                }
            }
            // rate change is exactly 1.0 -> redemption rate did not change
            Ordering::Equal => {
                info!(target: UPDATE_PHASE, "redemption rate epoch delta = 0.0");
            }
            // rate change is greater than 1.0 -> redemption rate increased
            Ordering::Greater => {
                let rate_delta = rate_change_decimal - Decimal::one();
                info!(target: UPDATE_PHASE, "redemption rate epoch delta = +{rate_delta}");

                let increment_threshold = Decimal::bps(self.cfg.ethereum.max_rate_increment_bps);
                if rate_delta > increment_threshold {
                    warn!(target: UPDATE_PHASE, "rate delta exceeds the threshold of {increment_threshold}; pausing the vault");
                    let pause_request = one_way_vault_contract.pause().into_transaction_request();
                    let pause_vault_exec_response =
                        self.eth_client.sign_and_send(pause_request).await?;
                    eth_rp
                        .get_transaction_receipt(pause_vault_exec_response.transaction_hash)
                        .await?;

                    return Err(anyhow!(
                        "newly calculated rate exceeds the rate update thresholds"
                    ));
                }
            }
        }

        Ok(())
    }

    async fn total_issued_shares(&self, eth_rp: &CustomProvider) -> anyhow::Result<u128> {
        let one_way_vault_contract =
            OneWayVault::new(self.cfg.ethereum.libraries.one_way_vault, &eth_rp);

        let eth_vault_issued_shares_u256 = self
            .eth_client
            .query(one_way_vault_contract.totalSupply())
            .await?
            ._0;

        // if there are no shares issued, update cannot be performed because it's impossible to
        // calculate the redemption rate
        if eth_vault_issued_shares_u256.is_zero() {
            return Err(anyhow!(
                "cannot calculate redemption rate with zero issued vault shares"
            ));
        }

        info!(target: UPDATE_PHASE, "eth_vault_issued_shares_u256={eth_vault_issued_shares_u256}");

        // perform u256 -> u128 conversion
        let eth_vault_issued_shares_u128 = u128::try_from(eth_vault_issued_shares_u256)?;

        Ok(eth_vault_issued_shares_u128)
    }

    async fn total_deposit_assets(&self, eth_rp: &CustomProvider) -> anyhow::Result<u128> {
        let eth_deposit_acc_contract =
            BaseAccount::new(self.cfg.ethereum.accounts.deposit, &eth_rp);
        let eth_deposit_denom_contract =
            ERC20::new(self.cfg.ethereum.denoms.deposit_token, &eth_rp);

        let mut deposit_token_balance_total: u128 = 0;

        let eth_deposit_acc_balance_u256 = self
            .eth_client
            .query(eth_deposit_denom_contract.balanceOf(*eth_deposit_acc_contract.address()))
            .await?
            ._0;
        info!(target: UPDATE_PHASE, "eth_deposit_acc_balance_u256={eth_deposit_acc_balance_u256}");

        // perform u256 -> u128 conversion
        let eth_deposit_token_total_u128 = u128::try_from(eth_deposit_acc_balance_u256)?;
        info!(target: UPDATE_PHASE, "eth_deposit_token_total_u128={eth_deposit_token_total_u128}");
        deposit_token_balance_total += eth_deposit_token_total_u128;

        let noble_acc_balance = self
            .noble_client
            .query_balance(
                &self.cfg.noble.forwarding_account,
                &self.cfg.noble.chain_denom,
            )
            .await?;
        warn!(target: UPDATE_PHASE, "noble_fwd_account_balance={noble_acc_balance} . funds need manual routing Noble -> Neutron!");
        deposit_token_balance_total += noble_acc_balance;

        let neutron_deposit_acc_balance = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.deposit,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
        info!(target: UPDATE_PHASE, "neutron_deposit_acc_balance={neutron_deposit_acc_balance}");
        deposit_token_balance_total += neutron_deposit_acc_balance;

        let supervaults_tvl = Strategy::query_supervault_tvl_expressed_in_denom(
            &self.neutron_client,
            &self.cfg.neutron.supervault,
            &self.cfg.neutron.accounts.deposit,
            &self.cfg.neutron.accounts.settlement,
            &self.cfg.neutron.denoms.deposit_token,
        )
        .await?;
        info!(target: UPDATE_PHASE, "supervaults_tvl={supervaults_tvl}");
        deposit_token_balance_total += supervaults_tvl;

        Ok(deposit_token_balance_total)
    }
}
