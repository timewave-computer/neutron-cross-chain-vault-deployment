use crate::strategy_config::Strategy;
use alloy::{primitives::U256, providers::Provider};
use anyhow::anyhow;
use cosmwasm_std::{Decimal, Uint128};
use log::info;
use packages::utils::maxbtc::query_maxbtc_er;
use packages::{
    phases::UPDATE_PHASE,
    types::sol_types::{BaseAccount, ERC20, OneWayVault},
    utils,
    utils::{maxbtc::query_maxbtc_simulate_deposit, valence_core},
};
use valence_domain_clients::{
    cosmos::base_client::BaseClient,
    evm::base_client::{CustomProvider, EvmBaseClient},
};

impl Strategy {
    /// performs the vault rate update. this phase involves the following stages:
    /// 1. calculating the total amount of deposit assets distributed across all
    ///    active program domain accounts
    /// 2. convert these assets to their equivalent value in maxBTC
    /// 3. add this to the maxBTC in the settlement account
    /// 4. querying the shares issued by the vault on Ethereum
    /// 5. calculating the new redemption rate by dividing the total maxBTC
    ///    amount by the total shares
    /// 6. validating the new redemption rate
    /// 7. posting the updated rate to the Ethereum vault
    pub async fn update(&mut self, eth_rp: &CustomProvider) -> anyhow::Result<()> {
        info!(target: UPDATE_PHASE, "starting vault update phase");

        let one_way_vault_contract =
            OneWayVault::new(self.cfg.ethereum.libraries.one_way_vault, &eth_rp);

        // in order to calculate the vault rate we need to find the total amount of deposit
        // denom distributed across the program and convert it to the equivalent maxBTC value
        let total_assets_in_wbtc = self.total_assets_in_wbtc(eth_rp).await?;
        info!(target: UPDATE_PHASE, "total assets in wBTC: {total_assets_in_wbtc}");

        // fetch the total issued shares and convert them to u128
        let total_shares = self.total_issued_shares(eth_rp).await?;
        info!(target: UPDATE_PHASE, "eth_vault_issued_shares_u128={total_shares}");

        // rate = effective_total_assets_in_wBTC / (effective_vault_shares * scaling_factor)
        // multiplying the denominator by the scaling factor
        let scaled_shares_amount =
            Uint128::from(total_shares).checked_mul(self.cfg.ethereum.rate_scaling_factor)?;
        let redemption_rate_decimal =
            Decimal::checked_from_ratio(total_assets_in_wbtc, scaled_shares_amount)?;
        info!(target: UPDATE_PHASE, "redemption rate decimal={redemption_rate_decimal}");

        let redemption_rate_sol_u256 = U256::try_from(redemption_rate_decimal.atomics().u128())?;
        info!(target: UPDATE_PHASE, "redemption_rate_sol_u256={redemption_rate_sol_u256}");

        // validate that the newly calculated redemption rate does not exceed
        // the max rate update thresholds relative to the current rate
        valence_core::validate_new_redemption_rate(
            self.cfg.ethereum.libraries.one_way_vault,
            &self.eth_client,
            eth_rp,
            redemption_rate_sol_u256,
            self.cfg.ethereum.max_rate_decrement_bps,
            self.cfg.ethereum.max_rate_increment_bps,
        )
        .await?;

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

    /// queries the total value of the vault, expressed in maxBTC
    /// this involves querying the deposit token balances as well as the maxBTC balance
    /// in the supervault deposit account, and convert everything to wBTC
    /// - deposit denom balance queries:
    ///   - ethereum deposit account
    ///   - cosmos hub ICA
    ///   - neutron deposit account
    /// - maxBTC balance queries:
    ///   - neutron settlement account
    async fn total_assets_in_wbtc(&self, eth_rp: &CustomProvider) -> anyhow::Result<u128> {
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

        let gaia_ica_balance = self
            .gaia_client
            .query_balance(&self.cfg.gaia.ica_address, &self.cfg.gaia.deposit_denom)
            .await?;
        info!(target: UPDATE_PHASE, "gaia_ica_balance={gaia_ica_balance}");
        deposit_token_balance_total += gaia_ica_balance;

        let neutron_deposit_acc_balance = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.ica_deposit,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
        info!(target: UPDATE_PHASE, "neutron_deposit_acc_balance={neutron_deposit_acc_balance}");
        deposit_token_balance_total += neutron_deposit_acc_balance;

        let neutron_supervault_deposit_acc_maxbtc_balance = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.supervault_deposit,
                &self.cfg.neutron.denoms.maxbtc,
            )
            .await?;
        info!(target: UPDATE_PHASE, "neutron_supervault_deposit_acc_maxbtc_balance={neutron_supervault_deposit_acc_maxbtc_balance}");
        let current_maxbtc_exchange_rate: Decimal =
            query_maxbtc_er(&self.neutron_client, &self.cfg.neutron.maxbtc_contract).await?;

        // Calculate the final amount of maxBTC to be minted after fees and exchange rate conversion
        let neutron_supervault_deposit_acc_balance_in_wbtc = dec_to_amount(
            Decimal::from_atomics(neutron_supervault_deposit_acc_maxbtc_balance, 8)?
                * current_maxbtc_exchange_rate,
            8,
        )?;
        deposit_token_balance_total += neutron_supervault_deposit_acc_balance_in_wbtc.u128();

        let supervaults_tvl = utils::supervaults::query_supervault_tvl_expressed_in_denom(
            &self.neutron_client,
            &self.cfg.neutron.supervault_contract,
            &self.cfg.neutron.accounts.supervault_deposit,
            &self.cfg.neutron.accounts.settlement,
            &self.cfg.neutron.denoms.deposit_token,
        )
        .await?;
        info!(target: UPDATE_PHASE, "supervaults_tvl={supervaults_tvl}");

        deposit_token_balance_total += supervaults_tvl;
        info!(target: UPDATE_PHASE, "deposit_token_balance_total={deposit_token_balance_total}");

        Ok(deposit_token_balance_total)
    }
}

pub fn dec_to_amount(dec: Decimal, decimals: u32) -> anyhow::Result<Uint128> {
    dec.atomics()
        .checked_div(Uint128::from(10u128.pow(dec.decimal_places() - decimals)))
        .map_err(anyhow::Error::from)
}
