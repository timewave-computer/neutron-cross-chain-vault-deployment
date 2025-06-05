use std::{error::Error, str::FromStr};

use alloy::{primitives::U256, providers::Provider};
use async_trait::async_trait;
use cosmwasm_std::{Decimal, Uint128, Uint256};
use log::info;
use types::{
    labels::{
        ICA_TRANSFER_LABEL, MARS_LEND_LABEL, MARS_WITHDRAW_LABEL, REGISTER_OBLIGATION_LABEL,
        SETTLE_OBLIGATION_LABEL,
    },
    sol_types::{
        BaseAccount, ERC20,
        OneWayVault::{self, WithdrawRequested},
    },
};
use valence_clearing_queue_supervaults::msg::ObligationsResponse;
use valence_domain_clients::{
    cosmos::{base_client::BaseClient, wasm_client::WasmClient},
    evm::{
        base_client::{CustomProvider, EvmBaseClient},
        request_provider_client::RequestProviderClient,
    },
    indexer::one_way_vault::OneWayVaultIndexer,
};
use valence_strategist_utils::worker::ValenceWorker;

use crate::strategy_config::Strategy;

const SCALING_FACTOR: u128 = 1_000_000_000_000;

// implement the ValenceWorker trait for the Strategy struct.
// This trait defines the main loop of the strategy and inherits
// the default implementation for spawning the worker.
#[async_trait]
impl ValenceWorker for Strategy {
    fn get_name(&self) -> String {
        format!("Valence X-Vault: {}", self.label)
    }

    async fn cycle(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!("{}: Starting cycle...", self.get_name());

        let eth_rp: CustomProvider = self.eth_client.get_request_provider().await?;

        // first we carry out the deposit flow
        self.deposit(&eth_rp).await?;

        // after deposit flow is complete, we process the new obligations
        self.register_withdraw_obligations().await?;

        // with new obligations registered into the clearing queue, we
        // carry out the settlements
        self.settlement().await?;

        // having processed all new exit requests after the deposit flow,
        // the epoch is ready to be concluded.
        // we perform the final accounting flow and post vault update.
        self.update(&eth_rp).await?;

        Ok(())
    }
}

impl Strategy {
    /// performs the vault rate update
    async fn update(
        &mut self,
        eth_rp: &CustomProvider,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let eth_deposit_acc_contract =
            BaseAccount::new(self.cfg.ethereum.accounts.deposit, &eth_rp);
        let one_way_vault_contract =
            OneWayVault::new(self.cfg.ethereum.libraries.one_way_vault, &eth_rp);
        let eth_deposit_denom_contract =
            ERC20::new(self.cfg.ethereum.denoms.deposit_token, &eth_rp);

        let eth_deposit_acc_balance = self
            .eth_client
            .query(eth_deposit_denom_contract.balanceOf(*eth_deposit_acc_contract.address()))
            .await?
            ._0;
        let eth_deposit_token_total_uint256 =
            Uint256::from_be_bytes(eth_deposit_acc_balance.to_be_bytes());
        let eth_deposit_token_total_uint128 =
            Uint128::from_str(&eth_deposit_token_total_uint256.to_string())?;

        let eth_vault_issued_shares = self
            .eth_client
            .query(one_way_vault_contract.totalSupply())
            .await?
            ._0;
        let eth_vault_issued_shares_uint256 =
            Uint256::from_be_bytes(eth_vault_issued_shares.to_be_bytes());
        let eth_vault_issued_shares_uint128 =
            Uint128::from_str(&eth_vault_issued_shares_uint256.to_string())?;

        let gaia_ica_balance = self
            .gaia_client
            .query_balance(&self.cfg.gaia.ica_address, &self.cfg.gaia.deposit_denom)
            .await?;

        let neutron_deposit_acc_balance = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.deposit,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;

        let neutron_settlement_acc_deposit_token_balance = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;

        // both mars and supervaults positions are derivatives of the
        // underlying denom. we do the necessary accounting for both and
        // fetch the tvl expressed in the underlying deposit token.
        let mars_tvl = self.mars_accounting().await?;
        let supervaults_tvl = self.supervaults_accounting().await?;

        // sum all deposit assets
        let deposit_token_total: u128 = [
            mars_tvl,
            supervaults_tvl,
            gaia_ica_balance,
            neutron_deposit_acc_balance,
            neutron_settlement_acc_deposit_token_balance,
            eth_deposit_token_total_uint128.u128(),
        ]
        .iter()
        .sum();

        // rate =  effective_total_assets / (effective_vault_shares * scaling_factor)
        let redemption_rate_decimal = Decimal::from_ratio(
            deposit_token_total,
            // multiplying the denominator by the scaling factor
            // TODO: check if this scaling factor makes sense
            eth_vault_issued_shares_uint128.checked_mul(SCALING_FACTOR.into())?,
        );

        let redemption_rate_sol_u256 = U256::from(redemption_rate_decimal.atomics().u128());

        let update_tx = one_way_vault_contract.update(redemption_rate_sol_u256);

        let update_result = self
            .eth_client
            .execute_tx(update_tx.into_transaction_request())
            .await?;

        eth_rp
            .get_transaction_receipt(update_result.transaction_hash)
            .await?;

        Ok(())
    }

    /// calculates total value of everything related to the Mars flow:
    /// - mars input account
    /// - mars position
    ///
    /// returns total amount expressed in the deposit token
    async fn mars_accounting(&mut self) -> Result<u128, Box<dyn Error + Send + Sync>> {
        let neutron_mars_deposit_acc_balance = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.mars_deposit,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;

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

        // extract the credit account id. while credit accounts are returned as a vec,
        // mars lending library should only ever create one credit account and re-use it
        // for all LP actions, so we get the [0]
        let mars_input_credit_account_id = mars_input_acc_credit_accounts[0].id.to_string();

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

        // find the relevant denom among the active lends
        let mut mars_lending_deposit_token_amount = Uint128::zero();
        for lend in mars_positions_response.lends {
            if lend.denom == self.cfg.neutron.denoms.deposit_token {
                mars_lending_deposit_token_amount = lend.amount;
            }
        }

        let total_mars_value =
            neutron_mars_deposit_acc_balance + mars_lending_deposit_token_amount.u128();

        Ok(total_mars_value)
    }

    /// calculates total value of everything related to the Supervaults flow:
    /// - supervaults input account
    /// - supervaults LP shares (settlement account)
    ///
    /// returns total amount expressed in the deposit token
    async fn supervaults_accounting(&mut self) -> Result<u128, Box<dyn Error + Send + Sync>> {
        let neutron_supervault_acc_balance = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.supervault_deposit,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;

        let neutron_settlement_acc_lp_token_balance = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.supervault_lp,
            )
            .await?;

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
                    amount: neutron_settlement_acc_lp_token_balance.into(),
                },
            )
            .await?;

        // TODO: validate whether this logic is correct. Depending
        // on whether withdraw simulation results in both vault assets
        // or just the one that was deposited, the matching should is
        // done differently. If it turns out that both assets are returned,
        // there are two options:
        // 1. simulate the non-deposit token liquidation for the deposit token (safe)
        // 2. multiply the deposit token amount by 2 (naive, assuming liquidation
        // returns both assets of equal value)
        let simulate_withdraw_deposit_token = if self
            .cfg
            .neutron
            .denoms
            .deposit_token
            .eq(&supervault_cfg.pair_data.token_0.denom)
        {
            withdraw_amount_0
        } else if self
            .cfg
            .neutron
            .denoms
            .deposit_token
            .eq(&supervault_cfg.pair_data.token_1.denom)
        {
            withdraw_amount_1
        } else {
            Uint128::zero()
        };

        let total_supervaults_assets =
            simulate_withdraw_deposit_token.u128() + neutron_supervault_acc_balance;

        Ok(total_supervaults_assets)
    }

    /// carries out the steps needed to bring the new deposits from Ethereum to
    /// Neutron (via Cosmos Hub) before depositing them into Mars protocol.
    async fn deposit(
        &mut self,
        eth_rp: &CustomProvider,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let eth_wbtc_contract = ERC20::new(self.cfg.ethereum.denoms.deposit_token, &eth_rp);
        let eth_deposit_acc = BaseAccount::new(self.cfg.ethereum.accounts.deposit, &eth_rp);

        // 1. query the ethereum deposit account balance
        let eth_deposit_acc_bal = self
            .eth_client
            .query(eth_wbtc_contract.balanceOf(*eth_deposit_acc.address()))
            .await?
            ._0;

        // 2. validate that the deposit account balance exceeds the eureka routing
        // threshold amount
        if eth_deposit_acc_bal < self.cfg.ethereum.ibc_transfer_threshold_amt {
            // early return if balance is too small for the eureka transfer
            // to be worth it
            return Ok(());
        }

        // 3. perform IBC-Eureka transfer to Cosmos Hub ICA

        // 4. block execution until the funds arrive to the Cosmos Hub ICA owned
        // by the Valence Interchain Account on Neutron
        // TODO: doublecheck the precision conversion here
        let gaia_ica_balance = Uint128::from_str(&eth_deposit_acc_bal.to_string())?;

        let _gaia_ica_bal = self
            .gaia_client
            .poll_until_expected_balance(
                &self.cfg.gaia.ica_address,
                &self.cfg.gaia.deposit_denom,
                gaia_ica_balance.u128(),
                5,
                10,
            )
            .await?;

        // 5. enqueue:
        // - TODO: gaia ICA transfer update
        // - gaia ICA transfer
        self.enqueue_neutron(
            ICA_TRANSFER_LABEL,
            vec![valence_ica_ibc_transfer::msg::FunctionMsgs::Transfer {}],
        )
        .await?;

        self.tick_neutron().await?;

        // 6. block execution until funds arrive to the Neutron program deposit
        // account
        self.neutron_client
            .poll_until_expected_balance(
                &self.cfg.neutron.accounts.deposit,
                &self.cfg.neutron.denoms.deposit_token,
                gaia_ica_balance.u128(),
                5,
                10,
            )
            .await?;

        // 7. use Valence Forwarder to route funds from the Neutron program
        // deposit account to the Mars deposit account
        // 8. use Mars Lending library to deposit funds from Mars deposit account
        // into Mars protocol
        self.enqueue_neutron(
            MARS_LEND_LABEL,
            vec![
                valence_forwarder_library::msg::FunctionMsgs::Forward {},
                // valence_mars_lending::msg::FunctionMsgs::Lend {},
            ],
        )
        .await?;

        self.tick_neutron().await?;

        Ok(())
    }

    /// reads the newly submitted withdrawal obligations that are not yet
    /// present in the Clearing Queue, generates their zero-knowledge proofs,
    /// and posts them into the Clearing queue in order.
    async fn register_withdraw_obligations(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // 1. query the Clearing Queue library for the latest posted withdraw request ID
        let clearing_queue_cfg: valence_clearing_queue_supervaults::msg::Config = self
            .neutron_client
            .query_contract_state(
                &self.cfg.neutron.libraries.clearing_queue,
                valence_clearing_queue_supervaults::msg::QueryMsg::GetLibraryConfig {},
            )
            .await?;

        // 2. get id of the latest obligation request that was registered on neutron
        let latest_registered_obligation_id = clearing_queue_cfg.latest_id.u64();

        // 3. query the OneWayVault indexer to fetch all obligations that were registered
        // on the vault but are not yet registered into the queue on Neutron
        let new_obligations = self
            .indexer_client
            .query_vault_withdraw_requests(Some(latest_registered_obligation_id + 1))
            .await?;

        // 4. process the new OneWayVault Withdraw events in order from the oldest
        // to the newest, posting them to the coprocessor to obtain a ZKP
        for (obligation_id, owner, ntrn_receiver, shares) in new_obligations {
            let _withdraw_requested = WithdrawRequested {
                id: obligation_id,
                owner,
                receiver: ntrn_receiver,
                shares,
            };

            // TODO: post the request to coprocessor and get the response

            // 6. preserving the order, post the obligation built above to the
            // Neutron Authorizations contract, enqueuing them to the processor
            self.enqueue_neutron(REGISTER_OBLIGATION_LABEL, vec!["TODO"])
                .await?;

            // 7. tick the processor to register the obligations to the clearing queue
            self.tick_neutron().await?;
        }

        Ok(())
    }

    /// performs the final settlement of registered withdrawal obligations in
    /// the Clearing Queue library. this involves topping up the settlement
    /// account with funds necessary to carry out all withdrawal obligations
    /// in the queue.
    async fn settlement(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // 1. query the current settlement account balance
        let settlement_acc_bal = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;

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

        // sum the total obligations amount
        let total_queue_obligations: u128 = clearing_queue
            .obligations
            .iter()
            .map(|o| o.payout_coin.amount.u128())
            .sum();

        // 3. if settlement account balance is insufficient to cover the active
        // obligations, we perform the Mars protocol withdrawals
        if settlement_acc_bal < total_queue_obligations {
            // 3. simulate Mars protocol withdrawal to obtain the funds necessary
            // to fulfill all active withdrawal requests
            let obligations_delta = total_queue_obligations - settlement_acc_bal;

            // 4. call the Mars lending library to perform the withdrawal.
            // This will deposit the underlying assets directly to the settlement account.
            self.enqueue_neutron(
                MARS_WITHDRAW_LABEL,
                vec![&valence_mars_lending::msg::FunctionMsgs::Withdraw {
                    amount: Some(obligations_delta.into()),
                }],
            )
            .await?;

            self.tick_neutron().await?;
        }

        // 5. process the Clearing Queue settlement requests by enqueuing the settlement
        // messages to the processor and ticking
        for _ in clearing_queue.obligations {
            self.enqueue_neutron(
                SETTLE_OBLIGATION_LABEL,
                vec![
                    valence_clearing_queue_supervaults::msg::FunctionMsgs::SettleNextObligation {},
                ],
            )
            .await?;

            self.tick_neutron().await?;
        }

        Ok(())
    }
}
