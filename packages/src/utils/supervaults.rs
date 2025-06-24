use std::error::Error;

use async_trait::async_trait;
use cosmwasm_std::{Addr, Decimal, Uint128};
use mmvault::state::Config;
use valence_domain_clients::{
    clients::neutron::NeutronClient,
    cosmos::{base_client::BaseClient, wasm_client::WasmClient},
};

#[async_trait]
pub trait Supervaults {
    /// calculates total value of the active supervault position,
    /// expressed in the deposit token denom
    async fn query_supervault_tvl_expressed_in_denom(
        client: &NeutronClient,
        supervault: &str,
        deposit_acc: &str,
        settlement_acc: &str,
        deposit_denom: &str,
    ) -> Result<u128, Box<dyn Error + Send + Sync>> {
        // query the supervault config to get the pair denom ordering
        let supervault_cfg = Self::query_supervault_cfg(client, supervault).await?;

        // query the settlement acc lp token balance
        let lp_shares_balance: Uint128 = client
            .query_balance(settlement_acc, &supervault_cfg.lp_denom)
            .await?
            .into();

        // if no shares are available, we early return supervaults tvl of 0
        if lp_shares_balance.is_zero() {
            return Ok(0);
        }

        // simulate the liquidation of all LP shares owned by the settlement account.
        // this simulation returns a tuple of expected asset amounts, in order.
        let (withdraw_amount_0, withdraw_amount_1) =
            Self::simulate_supervault_withdraw_liquidity(client, supervault, lp_shares_balance)
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
        let exchange_rate = if deposit_denom == supervault_cfg.pair_data.token_0.denom {
            // simulate LP with the deposit token. amount here does not really matter,
            // but to avoid some rounding errors with small amounts we pass the expected
            // withdraw amount to get a reasonable value.
            let expected_lp_shares = Self::simulate_supervault_provide_liquidity(
                client,
                supervault,
                deposit_acc,
                withdraw_amount_0,
                Uint128::zero(),
            )
            .await?;

            Decimal::from_ratio(withdraw_amount_0, expected_lp_shares)
        } else {
            let expected_lp_shares = Self::simulate_supervault_provide_liquidity(
                client,
                supervault,
                deposit_acc,
                Uint128::zero(),
                withdraw_amount_1,
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

    async fn query_supervault_cfg(
        client: &NeutronClient,
        supervault: &str,
    ) -> Result<Config, Box<dyn Error + Send + Sync>> {
        let supervault_cfg: Config = client
            .query_contract_state(supervault, mmvault::msg::QueryMsg::GetConfig {})
            .await?;

        Ok(supervault_cfg)
    }

    async fn simulate_supervault_withdraw_liquidity(
        client: &NeutronClient,
        supervault: &str,
        shares: Uint128,
    ) -> Result<(Uint128, Uint128), Box<dyn Error + Send + Sync>> {
        // simulate the liquidation of all LP shares owned by the settlement account.
        // this simulation returns a tuple of expected asset amounts, in order.
        let resp: (Uint128, Uint128) = client
            .query_contract_state(
                supervault,
                mmvault::msg::QueryMsg::SimulateWithdrawLiquidity { amount: shares },
            )
            .await?;

        Ok(resp)
    }

    async fn simulate_supervault_provide_liquidity(
        client: &NeutronClient,
        supervault: &str,
        depositor: &str,
        amount_0: Uint128,
        amount_1: Uint128,
    ) -> Result<Uint128, Box<dyn Error + Send + Sync>> {
        // simulate LP with the deposit token. amount here does not really matter,
        // but to avoid some rounding errors with small amounts we pass the expected
        // withdraw amount to get a reasonable value.
        let expected_lp_shares: Uint128 = client
            .query_contract_state(
                supervault,
                mmvault::msg::QueryMsg::SimulateProvideLiquidity {
                    amount_0,
                    amount_1,
                    sender: Addr::unchecked(depositor.to_string()),
                },
            )
            .await?;

        Ok(expected_lp_shares)
    }
}
