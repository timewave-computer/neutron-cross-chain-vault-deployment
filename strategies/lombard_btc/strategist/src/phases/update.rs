use std::{error::Error, str::FromStr};

use alloy::{primitives::U256, providers::Provider};
use cosmwasm_std::{Addr, Decimal, Uint128, Uint256};
use log::{info, trace};
use packages::{
    phases::UPDATE_PHASE,
    types::sol_types::{BaseAccount, ERC20, OneWayVault},
};
use valence_domain_clients::{
    cosmos::{base_client::BaseClient, wasm_client::WasmClient},
    evm::base_client::{CustomProvider, EvmBaseClient},
};

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

        let eth_deposit_token_total_uint256 =
            Uint256::from_be_bytes(eth_deposit_acc_balance.to_be_bytes());
        let eth_deposit_token_total_uint128 =
            Uint128::from_str(&eth_deposit_token_total_uint256.to_string())?;
        info!(target: UPDATE_PHASE, "eth_deposit_acc_balance_u128={eth_deposit_token_total_uint128}");

        let eth_vault_issued_shares = self
            .eth_client
            .query(one_way_vault_contract.totalSupply())
            .await?
            ._0;
        info!(target: UPDATE_PHASE, "eth_vault_issued_shares={eth_vault_issued_shares}");
        let eth_vault_issued_shares_uint256 =
            Uint256::from_be_bytes(eth_vault_issued_shares.to_be_bytes());
        let eth_vault_issued_shares_uint128 =
            Uint128::from_str(&eth_vault_issued_shares_uint256.to_string())?;
        info!(target: UPDATE_PHASE, "eth_vault_issued_shares_uint128={eth_vault_issued_shares_uint128}");

        let gaia_ica_balance = self
            .gaia_client
            .query_balance(&self.cfg.gaia.ica_address, &self.cfg.gaia.deposit_denom)
            .await?;
        info!(target: UPDATE_PHASE, "gaia_ica_balance={gaia_ica_balance}");

        let neutron_deposit_acc_balance = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.deposit,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
        info!(target: UPDATE_PHASE, "neutron_deposit_acc_balance={neutron_deposit_acc_balance}");

        let neutron_settlement_acc_deposit_token_balance = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
        info!(target: UPDATE_PHASE, "neutron_settlement_acc_deposit_token_balance={neutron_settlement_acc_deposit_token_balance}");

        let neutron_mars_deposit_acc_balance = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.mars_deposit,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
        info!(target: UPDATE_PHASE, "neutron_mars_deposit_acc_balance={neutron_mars_deposit_acc_balance}");

        let neutron_supervault_acc_balance = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.supervault_deposit,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;
        info!(target: UPDATE_PHASE, "neutron_supervault_acc_balance={neutron_supervault_acc_balance}");

        // both mars and supervaults positions are derivatives of the
        // underlying denom. we do the necessary accounting for both and
        // fetch the tvl expressed in the underlying deposit token.
        let mars_tvl = self.mars_accounting().await?;
        info!(target: UPDATE_PHASE, "mars_tvl={mars_tvl}");

        let supervaults_tvl = self.supervaults_accounting().await?;
        info!(target: UPDATE_PHASE, "supervaults_tvl={supervaults_tvl}");

        // sum all deposit assets
        let deposit_token_total: u128 = [
            neutron_supervault_acc_balance,
            neutron_mars_deposit_acc_balance,
            mars_tvl,
            supervaults_tvl,
            gaia_ica_balance,
            neutron_deposit_acc_balance,
            neutron_settlement_acc_deposit_token_balance,
            eth_deposit_token_total_uint128.u128(),
        ]
        .iter()
        .sum();
        info!(target: UPDATE_PHASE, "deposit token total amount={deposit_token_total}");
        info!(target: UPDATE_PHASE, "rate_scaling_factor = {}", self.cfg.ethereum.rate_scaling_factor);

        // rate =  effective_total_assets / (effective_vault_shares * scaling_factor)
        let redemption_rate_decimal = Decimal::from_ratio(
            deposit_token_total,
            // multiplying the denominator by the scaling factor
            eth_vault_issued_shares_uint128.checked_mul(self.cfg.ethereum.rate_scaling_factor)?,
        );
        info!(target: UPDATE_PHASE, "redemption rate decimal={redemption_rate_decimal}");

        let redemption_rate_sol_u256 = U256::from(redemption_rate_decimal.atomics().u128());
        info!(target: UPDATE_PHASE, "redemption_rate_sol_u256={redemption_rate_sol_u256}");

        if redemption_rate_sol_u256 > current_vault_rate {
            let change_decimal = Decimal::from_ratio(
                Uint128::from_str(&redemption_rate_sol_u256.to_string()).unwrap(),
                Uint128::from_str(&current_vault_rate.to_string()).unwrap(),
            );

            let rate_delta = change_decimal - Decimal::one();
            info!(target: UPDATE_PHASE, "redemption rate epoch delta = +{rate_delta}");
        } else {
            let change_decimal = Decimal::from_ratio(
                Uint128::from_str(&redemption_rate_sol_u256.to_string()).unwrap(),
                Uint128::from_str(&current_vault_rate.to_string()).unwrap(),
            );
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

    /// calculates total value of the active Mars lending position,
    /// expressed in the deposit token
    async fn mars_accounting(&mut self) -> Result<u128, Box<dyn Error + Send + Sync>> {
        // query the mars credit account created and owned by the mars input account
        let mars_input_acc_credit_accounts: Vec<valence_lending_utils::mars::Account> = self
            .neutron_client
            .query_contract_state(
                &self.cfg.neutron.mars_pool,
                valence_lending_utils::mars::QueryMsg::Accounts {
                    owner: self.cfg.neutron.accounts.mars_deposit.to_string(),
                    start_after: None,
                    limit: None,
                },
            )
            .await?;

        info!(target: UPDATE_PHASE, "mars input credit accounts = {:?}", mars_input_acc_credit_accounts);

        // extract the credit account id. while credit accounts are returned as a vec,
        // mars lending library should only ever create one credit account and re-use it
        // for all LP actions, so we get the [0]
        let mars_input_credit_account_id = match mars_input_acc_credit_accounts.len() {
            // if this is the first cycle and no credit accounts exist,
            // we early return mars tvl 0
            0 => return Ok(0),
            _ => mars_input_acc_credit_accounts[0].id.to_string(),
        };

        info!(target: UPDATE_PHASE, "mars input credit account id = #{mars_input_credit_account_id}");

        // query mars positions owned by the credit account id
        let mars_positions_response: valence_lending_utils::mars::Positions = self
            .neutron_client
            .query_contract_state(
                &self.cfg.neutron.mars_pool,
                valence_lending_utils::mars::QueryMsg::Positions {
                    account_id: mars_input_credit_account_id,
                },
            )
            .await?;

        info!(target: UPDATE_PHASE, "mars credit account positions = {:?}", mars_positions_response);

        // find the relevant denom among the active lends
        let mut mars_lending_deposit_token_amount = Uint128::zero();
        for lend in mars_positions_response.lends {
            if lend.denom == self.cfg.neutron.denoms.deposit_token {
                mars_lending_deposit_token_amount = lend.amount;
            }
        }

        Ok(mars_lending_deposit_token_amount.u128())
    }

    /// calculates total value of the active supervault position,
    /// expressed in the deposit token denom
    async fn supervaults_accounting(&mut self) -> Result<u128, Box<dyn Error + Send + Sync>> {
        let lp_shares_balance: Uint128 = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.supervault_lp,
            )
            .await?
            .into();

        // if no shares are available, we early return supervaults tvl of 0
        if lp_shares_balance.is_zero() {
            return Ok(0);
        }

        // query the supervault config to get the pair denom ordering
        let supervault_cfg: mmvault::state::Config = self
            .neutron_client
            .query_contract_state(
                &self.cfg.neutron.supervault,
                mmvault::msg::QueryMsg::GetConfig {},
            )
            .await?;

        // simulate the liquidation of all LP shares owned by the settlement account.
        // this simulation returns a tuple of expected asset amounts, in order.
        let (withdraw_amount_0, withdraw_amount_1): (Uint128, Uint128) = self
            .neutron_client
            .query_contract_state(
                &self.cfg.neutron.supervault,
                mmvault::msg::QueryMsg::SimulateWithdrawLiquidity {
                    amount: lp_shares_balance,
                },
            )
            .await?;

        // the returned amounts above include a non-deposit denom which is not
        // relevant for our TVL calculation that is denominated in the deposit
        // denom. to deal with that, we need to express the LP shares entirely
        // in terms of the deposit denom.
        // we do this by:
        // 1. finding our deposit token withdraw amount
        // 2. simulating LP for that amount to know the deposit_token -> shares
        // exchange rate
        // 3. assuming the same rate for whole position
        let exchange_rate =
            if self.cfg.neutron.denoms.deposit_token == supervault_cfg.pair_data.token_0.denom {
                // simulate LP with the deposit token. amount here does not really matter,
                // but to avoid some rounding errors with small amounts we pass the expected
                // withdraw amount to get a reasonable value.
                let expected_lp_shares: Uint128 = self
                    .neutron_client
                    .query_contract_state(
                        &self.cfg.neutron.supervault,
                        mmvault::msg::QueryMsg::SimulateProvideLiquidity {
                            amount_0: withdraw_amount_0,
                            amount_1: Uint128::zero(),
                            sender: Addr::unchecked(
                                self.cfg.neutron.accounts.supervault_deposit.to_string(),
                            ),
                        },
                    )
                    .await?;

                Decimal::from_ratio(withdraw_amount_0, expected_lp_shares)
            } else {
                let expected_lp_shares: Uint128 = self
                    .neutron_client
                    .query_contract_state(
                        &self.cfg.neutron.supervault,
                        mmvault::msg::QueryMsg::SimulateProvideLiquidity {
                            amount_0: Uint128::zero(),
                            amount_1: withdraw_amount_1,
                            sender: Addr::unchecked(
                                self.cfg.neutron.accounts.supervault_deposit.to_string(),
                            ),
                        },
                    )
                    .await?;

                Decimal::from_ratio(withdraw_amount_1, expected_lp_shares)
            };

        // multiply the lp_shares balance by the derived (deposit_token / lp_shares) exchange rate
        // to get the lp_shares balance value expressed in deposit token denom
        let lp_shares_deposit_denom_equivalent =
            lp_shares_balance.checked_mul_floor(exchange_rate)?;

        Ok(lp_shares_deposit_denom_equivalent.u128())
    }
}
