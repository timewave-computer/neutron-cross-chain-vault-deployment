use std::{error::Error, str::FromStr, time::Duration};

use alloy::{
    primitives::{Bytes, U256, ruint::algorithms::gcd},
    providers::Provider,
};
use async_trait::async_trait;
use cosmwasm_std::{Addr, Decimal, Uint128, Uint256, to_json_binary};
use log::warn;
use log::{info, trace};
use serde_json::json;
use tokio::time::sleep;
use types::{
    labels::{
        ICA_TRANSFER_LABEL, LEND_AND_PROVIDE_LIQUIDITY_LABEL, MARS_WITHDRAW_LABEL,
        REGISTER_OBLIGATION_LABEL, SETTLE_OBLIGATION_LABEL,
    },
    sol_types::{
        Authorization, BaseAccount, ERC20,
        OneWayVault::{self},
    },
};
use valence_clearing_queue_supervaults::msg::ObligationsResponse;
use valence_domain_clients::{
    coprocessor::base_client::CoprocessorBaseClient,
    cosmos::{base_client::BaseClient, wasm_client::WasmClient},
    evm::{
        base_client::{CustomProvider, EvmBaseClient},
        request_provider_client::RequestProviderClient,
    },
    indexer::one_way_vault::OneWayVaultIndexer,
};
use valence_library_utils::OptionUpdate;
use valence_strategist_utils::worker::ValenceWorker;

use crate::strategy_config::Strategy;

// logging targets
const DEPOSIT_PHASE: &str = "deposit";
const UPDATE_PHASE: &str = "update";
const SETTLEMENT_PHASE: &str = "settlement";
const REGISTRATION_PHASE: &str = "registration";
const VALENCE_WORKER: &str = "valence_worker";

// implement the ValenceWorker trait for the Strategy struct.
// This trait defines the main loop of the strategy and inherits
// the default implementation for spawning the worker.
#[async_trait]
impl ValenceWorker for Strategy {
    fn get_name(&self) -> String {
        format!("Valence X-Vault: {}", self.label)
    }

    async fn cycle(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!(target: VALENCE_WORKER, "sleeping for {}sec", self.timeout);
        sleep(Duration::from_secs(self.timeout)).await;

        info!(target: VALENCE_WORKER, "{}: Starting cycle...", self.get_name());

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

        let update_tx = one_way_vault_contract.update(redemption_rate_sol_u256);

        info!(target: UPDATE_PHASE, "updating ethereum vault redemption rate");
        let update_result = self
            .eth_client
            .execute_tx(update_tx.into_transaction_request())
            .await?;

        eth_rp
            .get_transaction_receipt(update_result.transaction_hash)
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

    /// carries out the steps needed to bring the new deposits from Ethereum to
    /// Neutron (via Cosmos Hub) before depositing them into Mars protocol.
    async fn deposit(
        &mut self,
        eth_rp: &CustomProvider,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        trace!(target: DEPOSIT_PHASE, "starting deposit phase");

        let eth_wbtc_contract = ERC20::new(self.cfg.ethereum.denoms.deposit_token, &eth_rp);
        let eth_deposit_acc = BaseAccount::new(self.cfg.ethereum.accounts.deposit, &eth_rp);
        let eth_auth_contract = Authorization::new(self.cfg.ethereum.authorizations, &eth_rp);

        // 1. query the ethereum deposit account balance
        let eth_deposit_acc_bal = self
            .eth_client
            .query(eth_wbtc_contract.balanceOf(*eth_deposit_acc.address()))
            .await?
            ._0;
        info!(target: DEPOSIT_PHASE, "eth deposit acc balance = {eth_deposit_acc_bal}");

        // 2. validate that the deposit account balance exceeds the eureka routing
        // threshold amount
        if eth_deposit_acc_bal < self.cfg.ethereum.ibc_transfer_threshold_amt {
            warn!(target: DEPOSIT_PHASE, "eth deposit account balance does not meet the eureka transfer threshold; returning");
            // early return if balance is too small for the eureka transfer
            // to be worth it
            return Ok(());
        }

        // 3. fetch the IBC-Eureka route from eureka client
        let skip_api_response = match self
            .ibc_eureka_client
            .query_skip_eureka_route(eth_deposit_acc_bal.to_string())
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!(target: DEPOSIT_PHASE, "skip route error: {e}");
                return Ok(());
            }
        };

        // format the response in format expected by the coprocessor and post it
        // there for proof
        let coprocessor_input = json!({"skip_response": skip_api_response});
        info!(target: DEPOSIT_PHASE, "posting skip-api response to co-processor");
        let skip_response_zkp = self
            .coprocessor_client
            .prove(&self.cfg.coprocessor.eureka_circuit_id, &coprocessor_input)
            .await?;

        // extract the program and domain parameters by decoding the zkp
        let (proof_program, inputs_program) = skip_response_zkp.program.decode()?;
        let (proof_domain, inputs_domain) = skip_response_zkp.domain.decode()?;

        // build the eureka transfer zk message from decoded params
        let auth_eureka_transfer_zk_msg = eth_auth_contract.executeZKMessage(
            Bytes::from(inputs_program),
            Bytes::from(proof_program),
            Bytes::from(inputs_domain),
            Bytes::from(proof_domain),
        );

        // sign and execute the tx & await its tx receipt before proceeding
        info!(target: DEPOSIT_PHASE, "posting skip-api zkp ethereum authorizations");
        let zk_auth_exec_response = self
            .eth_client
            .sign_and_send(auth_eureka_transfer_zk_msg.into_transaction_request())
            .await?;
        eth_rp
            .get_transaction_receipt(zk_auth_exec_response.transaction_hash)
            .await?;

        // 4. block execution until the funds arrive to the Cosmos Hub ICA owned
        // by the Valence Interchain Account on Neutron
        // TODO: doublecheck the precision conversion here
        let gaia_ica_balance = Uint128::from_str(&eth_deposit_acc_bal.to_string())?;
        info!(target: DEPOSIT_PHASE, "gaia ica expected deposit token bal = {gaia_ica_balance}; starting to poll");

        let gaia_ica_bal = self
            .gaia_client
            .poll_until_expected_balance(
                &self.cfg.gaia.ica_address,
                &self.cfg.gaia.deposit_denom,
                gaia_ica_balance.u128(),
                5,
                10,
            )
            .await?;

        // 5. enqueue: gaia ICA transfer amount update & gaia ica transfer messages
        let ica_ibc_transfer_update_msg =
            to_json_binary(&valence_ica_ibc_transfer::msg::LibraryConfigUpdate {
                input_addr: None,
                amount: Some(gaia_ica_bal.into()),
                denom: None,
                receiver: None,
                memo: None,
                remote_chain_info: None,
                denom_to_pfm_map: None,
                eureka_config: OptionUpdate::None,
            })?;
        let ica_ibc_transfer_msg =
            to_json_binary(&valence_ica_ibc_transfer::msg::FunctionMsgs::Transfer {})?;

        info!(target: DEPOSIT_PHASE, "performing ica_ibc_transfer library update & transfer");
        self.enqueue_neutron(
            ICA_TRANSFER_LABEL,
            vec![ica_ibc_transfer_update_msg, ica_ibc_transfer_msg],
        )
        .await?;

        self.tick_neutron().await?;

        info!(target: DEPOSIT_PHASE, "polling for neutron deposit account to receive the funds");

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

        info!(target: DEPOSIT_PHASE, "routing funds from neutron deposit account to mars and supervaults for lending");

        // 7. use Splitter to route funds from the Neutron program
        // deposit account to the Mars and Supervaults deposit accounts
        let split_msg = to_json_binary(&valence_splitter_library::msg::FunctionMsgs::Split {})?;

        // 8. use Mars Lending library to deposit funds from Mars deposit account
        // into Mars protocol
        let mars_lend_msg = to_json_binary(&valence_mars_lending::msg::FunctionMsgs::Lend {})?;

        // 9. use Supervaults lper library to deposit funds from Supervaults deposit account
        // into the configured supervault
        let supervaults_lp_msg = to_json_binary(
            &valence_supervaults_lper::msg::FunctionMsgs::ProvideLiquidity {
                expected_vault_ratio_range: None,
            },
        )?;

        self.enqueue_neutron(
            LEND_AND_PROVIDE_LIQUIDITY_LABEL,
            vec![split_msg, mars_lend_msg, supervaults_lp_msg],
        )
        .await?;

        self.tick_neutron().await?;

        Ok(())
    }

    /// reads the newly submitted withdrawal obligations that are not yet
    /// present in the Clearing Queue, generates their zero-knowledge proofs,
    /// and posts them into the Clearing queue in order.
    async fn register_withdraw_obligations(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        trace!(target: REGISTRATION_PHASE, "starting withdraw obligation registration phase");

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
        info!(
            target: REGISTRATION_PHASE,
            "latest_registered_obligation_id={latest_registered_obligation_id}"
        );

        // 3. query the OneWayVault indexer to fetch all obligations that were registered
        // on the vault but are not yet registered into the queue on Neutron
        let new_obligations: Vec<(u64, alloy::primitives::Address, String, U256)> = self
            .indexer_client
            .query_vault_withdraw_requests(Some(latest_registered_obligation_id + 1))
            .await
            .unwrap_or_default();
        info!(
            target: REGISTRATION_PHASE,
            "new_obligations = {:#?}", new_obligations
        );

        // 4. process the new OneWayVault Withdraw events in order from the oldest
        // to the newest, posting them to the coprocessor to obtain a ZKP
        for (obligation_id, ..) in new_obligations {
            trace!(
                target: REGISTRATION_PHASE,
                "processing obligation_id={obligation_id}"
            );

            // build the json input for coprocessor client
            let withdraw_id_json = json!({"withdrawal_request_id": obligation_id});

            // 5. post the proof request to the coprocessor client & await
            info!(target: REGISTRATION_PHASE, "posting zkp");
            let vault_zkp_response = self
                .coprocessor_client
                .prove(&self.cfg.coprocessor.vault_circuit_id, &withdraw_id_json)
                .await?;
            info!(target: REGISTRATION_PHASE, "received zkp from co-processor");

            // extract the program and domain parameters by decoding the zkp
            let (proof_program, inputs_program) = vault_zkp_response.program.decode()?;
            let (proof_domain, inputs_domain) = vault_zkp_response.domain.decode()?;

            // need to set these values to correct ones, placeholding for now
            let execute_zk_authorization_msg =
                valence_authorization_utils::msg::PermissionlessMsg::ExecuteZkAuthorization {
                    label: REGISTER_OBLIGATION_LABEL.to_string(),
                    message: cosmwasm_std::Binary::from(inputs_program),
                    proof: cosmwasm_std::Binary::from(proof_program),
                    domain_message: cosmwasm_std::Binary::from(inputs_domain),
                    domain_proof: cosmwasm_std::Binary::from(proof_domain),
                };

            // 6. execute the zk authorization. this will perform the verification
            // and, if successful, push the msg to the processor
            info!(target: REGISTRATION_PHASE, "executing zk authorization");
            self.neutron_client
                .execute_wasm(
                    &self.cfg.neutron.authorizations,
                    valence_authorization_utils::msg::ExecuteMsg::PermissionlessAction(
                        execute_zk_authorization_msg,
                    ),
                    vec![],
                    None,
                )
                .await?;

            // 7. tick the processor to register the obligation to the clearing queue
            self.tick_neutron().await?;
        }

        Ok(())
    }

    /// performs the final settlement of registered withdrawal obligations in
    /// the Clearing Queue library. this involves topping up the settlement
    /// account with funds necessary to carry out all withdrawal obligations
    /// in the queue.
    async fn settlement(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        trace!(target: SETTLEMENT_PHASE, "starting settlement phase");

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
