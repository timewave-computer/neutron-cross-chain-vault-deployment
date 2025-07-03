use alloy::{primitives::U256, providers::Provider};
use cosmwasm_std::{Decimal, Uint128};
use log::{info};
use packages::{
    phases::UPDATE_PHASE,
    types::sol_types::{BaseAccount, ERC20, OneWayVault},
    utils::{mars::MarsLending, supervaults::Supervaults},
};
use valence_domain_clients::{
    cosmos::base_client::BaseClient,
    evm::base_client::{CustomProvider, EvmBaseClient},
};

use crate::strategy_config::Strategy;

impl Strategy {
    /// performs the vault rate update
    pub async fn update(&mut self, eth_rp: &CustomProvider) -> anyhow::Result<()> {
        info!(target: UPDATE_PHASE, "starting vault update phase");

        let mut total_deposit_assets = 0;

        let eth_deposit_acc_contract =
            BaseAccount::new(self.cfg.ethereum.accounts.deposit, &eth_rp);
        let one_way_vault_contract =
            OneWayVault::new(self.cfg.ethereum.libraries.one_way_vault, &eth_rp);
        let eth_deposit_denom_contract =
            ERC20::new(self.cfg.ethereum.denoms.deposit_token, &eth_rp);

        let current_vault_rate = self
            .eth_client
            .query(one_way_vault_contract.redemptionRate())
            .await?
            ._0;
        info!(target: UPDATE_PHASE, "pre_update_rate = {current_vault_rate}");

        let eth_deposit_acc_balance = self
            .eth_client
            .query(eth_deposit_denom_contract.balanceOf(*eth_deposit_acc_contract.address()))
            .await?
            ._0;
        info!(target: UPDATE_PHASE, "eth_deposit_acc_balance={eth_deposit_acc_balance}");

        let eth_deposit_token_total_u128: u128 = u128::try_from(eth_deposit_acc_balance)?;
        info!(target: UPDATE_PHASE, "eth_deposit_token_total_u128={eth_deposit_token_total_u128}");
        total_deposit_assets += eth_deposit_token_total_u128;

        let eth_vault_issued_shares = self
            .eth_client
            .query(one_way_vault_contract.totalSupply())
            .await?
            ._0;

        info!(target: UPDATE_PHASE, "eth_vault_issued_shares={eth_vault_issued_shares}");

        // if there are no shares issued, update cannot be performed because it's impossible to
        // calculate the redemption rate
        if eth_vault_issued_shares.is_zero() {
            return Err(anyhow::anyhow!(
                "cannot calculate redemption rate with zero issued vault shares"
            ));
        }

        // perform u256 -> u128 conversion
        let eth_vault_issued_shares_u128 = u128::try_from(eth_vault_issued_shares)?;
        info!(target: UPDATE_PHASE, "eth_vault_issued_shares_u128={eth_vault_issued_shares_u128}");

        // first step to rate calculation is to sum all cosmos accounts for
        // their deposit token holdings
        {
            let gaia_ica_balance = self
                .gaia_client
                .query_balance(&self.cfg.gaia.ica_address, &self.cfg.gaia.deposit_denom)
                .await?;
            info!(target: UPDATE_PHASE, "gaia_ica_balance={gaia_ica_balance}");
            total_deposit_assets += gaia_ica_balance;

            let neutron_deposit_acc_balance = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.deposit,
                    &self.cfg.neutron.denoms.deposit_token,
                )
                .await?;
            info!(target: UPDATE_PHASE, "neutron_deposit_acc_balance={neutron_deposit_acc_balance}");
            total_deposit_assets += neutron_deposit_acc_balance;
            let neutron_settlement_acc_deposit_token_balance = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.settlement,
                    &self.cfg.neutron.denoms.deposit_token,
                )
                .await?;
            info!(target: UPDATE_PHASE, "neutron_settlement_acc_deposit_token_balance={neutron_settlement_acc_deposit_token_balance}");
            total_deposit_assets += neutron_settlement_acc_deposit_token_balance;

            let neutron_mars_deposit_acc_balance = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.mars_deposit,
                    &self.cfg.neutron.denoms.deposit_token,
                )
                .await?;
            info!(target: UPDATE_PHASE, "neutron_mars_deposit_acc_balance={neutron_mars_deposit_acc_balance}");
            total_deposit_assets += neutron_mars_deposit_acc_balance;

            let neutron_supervaults_bedrockbtc_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.bedrockbtc_supervault_deposit,
                    &self.cfg.neutron.denoms.deposit_token,
                )
                .await?;
            info!(target: UPDATE_PHASE, "neutron_supervaults_bedrockbtc_bal={neutron_supervaults_bedrockbtc_bal}");
            total_deposit_assets += neutron_supervaults_bedrockbtc_bal;

            let neutron_supervaults_ebtc_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.ebtc_supervault_deposit,
                    &self.cfg.neutron.denoms.deposit_token,
                )
                .await?;
            info!(target: UPDATE_PHASE, "neutron_supervaults_ebtc_bal={neutron_supervaults_ebtc_bal}");
            total_deposit_assets += neutron_supervaults_ebtc_bal;

            let neutron_supervaults_fbtc_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.fbtc_supervault_deposit,
                    &self.cfg.neutron.denoms.deposit_token,
                )
                .await?;
            info!(target: UPDATE_PHASE, "neutron_supervaults_fbtc_bal={neutron_supervaults_fbtc_bal}");
            total_deposit_assets += neutron_supervaults_fbtc_bal;

            let neutron_supervaults_lbtc_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.lbtc_supervault_deposit,
                    &self.cfg.neutron.denoms.deposit_token,
                )
                .await?;
            info!(target: UPDATE_PHASE, "neutron_supervaults_lbtc_bal={neutron_supervaults_lbtc_bal}");
            total_deposit_assets += neutron_supervaults_lbtc_bal;

            let neutron_supervaults_pumpbtc_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.pumpbtc_supervault_deposit,
                    &self.cfg.neutron.denoms.deposit_token,
                )
                .await?;
            info!(target: UPDATE_PHASE, "neutron_supervaults_pumpbtc_bal={neutron_supervaults_pumpbtc_bal}");
            total_deposit_assets += neutron_supervaults_pumpbtc_bal;

            let neutron_supervaults_solvbtc_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.solvbtc_supervault_deposit,
                    &self.cfg.neutron.denoms.deposit_token,
                )
                .await?;
            info!(target: UPDATE_PHASE, "neutron_supervaults_solvbtc_bal={neutron_supervaults_solvbtc_bal}");
            total_deposit_assets += neutron_supervaults_solvbtc_bal;
        }

        // both mars and supervaults positions are derivatives of the
        // underlying denom. we do the necessary accounting for both and
        // fetch the tvl expressed in the underlying deposit token.
        {
            let mars_tvl = Strategy::query_mars_lending_denom_amount(
                &self.neutron_client,
                &self.cfg.neutron.mars_credit_manager,
                &self.cfg.neutron.accounts.mars_deposit,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
            info!(target: UPDATE_PHASE, "mars_tvl={mars_tvl}");
            total_deposit_assets += mars_tvl;

            let bedrockbtc_tvl = Strategy::query_supervault_tvl_expressed_in_denom(
                &self.neutron_client,
                &self.cfg.neutron.bedrockbtc_supervault,
                &self.cfg.neutron.accounts.bedrockbtc_supervault_deposit,
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
            info!(target: UPDATE_PHASE, "bedrockbtc_tvl={bedrockbtc_tvl}");
            total_deposit_assets += bedrockbtc_tvl;

            let ebtc_tvl = Strategy::query_supervault_tvl_expressed_in_denom(
                &self.neutron_client,
                &self.cfg.neutron.ebtc_supervault,
                &self.cfg.neutron.accounts.ebtc_supervault_deposit,
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
            info!(target: UPDATE_PHASE, "ebtc_tvl={ebtc_tvl}");
            total_deposit_assets += ebtc_tvl;

            let fbtc_tvl = Strategy::query_supervault_tvl_expressed_in_denom(
                &self.neutron_client,
                &self.cfg.neutron.fbtc_supervault,
                &self.cfg.neutron.accounts.fbtc_supervault_deposit,
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
            info!(target: UPDATE_PHASE, "fbtc_tvl={fbtc_tvl}");
            total_deposit_assets += fbtc_tvl;

            let lbtc_tvl = Strategy::query_supervault_tvl_expressed_in_denom(
                &self.neutron_client,
                &self.cfg.neutron.lbtc_supervault,
                &self.cfg.neutron.accounts.lbtc_supervault_deposit,
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
            info!(target: UPDATE_PHASE, "lbtc_tvl={lbtc_tvl}");
            total_deposit_assets += lbtc_tvl;

            let pumpbtc_tvl = Strategy::query_supervault_tvl_expressed_in_denom(
                &self.neutron_client,
                &self.cfg.neutron.pumpbtc_supervault,
                &self.cfg.neutron.accounts.pumpbtc_supervault_deposit,
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
            info!(target: UPDATE_PHASE, "pumpbtc_tvl={pumpbtc_tvl}");
            total_deposit_assets += pumpbtc_tvl;

            let solvbtc_tvl = Strategy::query_supervault_tvl_expressed_in_denom(
                &self.neutron_client,
                &self.cfg.neutron.solvbtc_supervault,
                &self.cfg.neutron.accounts.solvbtc_supervault_deposit,
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
            info!(target: UPDATE_PHASE, "solvbtc_tvl={solvbtc_tvl}");
            total_deposit_assets += solvbtc_tvl;
        }

        info!(target: UPDATE_PHASE, "total_deposit_assets={total_deposit_assets}");
        info!(target: UPDATE_PHASE, "rate_scaling_factor = {}", self.cfg.ethereum.rate_scaling_factor);

        // rate =  effective_total_assets / (effective_vault_shares * scaling_factor)
        let redemption_rate_decimal = Decimal::from_ratio(
            total_deposit_assets,
            // multiplying the denominator by the scaling factor
            Uint128::from(eth_vault_issued_shares_u128)
                .checked_mul(self.cfg.ethereum.rate_scaling_factor)?,
        );
        info!(target: UPDATE_PHASE, "redemption rate decimal={redemption_rate_decimal}");

        let redemption_rate_sol_u256 = U256::try_from(redemption_rate_decimal.atomics().u128())?;
        info!(target: UPDATE_PHASE, "redemption_rate_sol_u256={redemption_rate_sol_u256}");
        let redemption_rate_u128 = u128::try_from(redemption_rate_sol_u256)?;
        let current_rate_u128 = u128::try_from(current_vault_rate)?;

        if redemption_rate_sol_u256 > current_vault_rate {
            let change_decimal = Decimal::from_ratio(redemption_rate_u128, current_rate_u128);

            let rate_delta = change_decimal - Decimal::one();
            info!(target: UPDATE_PHASE, "redemption rate epoch delta = +{rate_delta}");
        } else {
            let change_decimal = Decimal::from_ratio(redemption_rate_u128, current_rate_u128);
            let rate_delta = Decimal::one() - change_decimal;

            info!(target: UPDATE_PHASE, "redemption rate epoch delta = -{rate_delta}");
        };

        info!(target: UPDATE_PHASE, "updating ethereum vault redemption rate");
        let update_request = one_way_vault_contract
            .update(redemption_rate_sol_u256)
            .into_transaction_request();

        let update_vault_exec_response = self.eth_client.sign_and_send(update_request).await?;

        eth_rp
            .get_transaction_receipt(update_vault_exec_response.transaction_hash)
            .await?;

        Ok(())
    }
}
