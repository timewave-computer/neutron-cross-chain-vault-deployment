use alloy::{
    primitives::{Bytes, U256},
    providers::Provider,
};
use std::thread::sleep;
use std::time::Duration;

use crate::strategy_config::Strategy;
use cosmwasm_std::to_json_binary;
use log::{info, warn};
use packages::{
    // Ensure SUPERVAULT_LP_LABEL is defined here
    labels::{ICA_TRANSFER_LABEL, MAXBTC_ISSUE_LABEL, PROVIDE_LIQUIDIY_LABEL},
    phases::DEPOSIT_PHASE,
    types::sol_types::{Authorization, BaseAccount, ERC20},
    utils::{self, valence_core},
};
use serde_json::json;
use valence_domain_clients::{
    coprocessor::base_client::CoprocessorBaseClient,
    cosmos::base_client::BaseClient,
    evm::base_client::{CustomProvider, EvmBaseClient},
};
use valence_library_utils::OptionUpdate;

impl Strategy {
    /// carries out the steps needed to bring the new deposits from Ethereum to
    /// Neutron (via Cosmos Hub) before minting maxBTC and providing liquidity.
    /// consists of four stages:
    /// 1. Ethereum -> Hub routing
    /// 2. Hub -> Neutron routing
    /// 3. maxBTC issuing
    /// 4. Supervault liquidity provision
    pub async fn deposit(&mut self, eth_rp: &CustomProvider) -> anyhow::Result<()> {
        info!(target: DEPOSIT_PHASE, "starting deposit phase");

        // Stage 1: deposit token routing from Ethereum to Cosmos hub
        {
            let eth_deposit_token_contract =
                ERC20::new(self.cfg.ethereum.denoms.deposit_token, &eth_rp);
            let eth_deposit_acc = BaseAccount::new(self.cfg.ethereum.accounts.deposit, &eth_rp);

            // query the ethereum deposit account balance
            let eth_deposit_acc_bal = self
                .eth_client
                .query(eth_deposit_token_contract.balanceOf(*eth_deposit_acc.address()))
                .await?
                ._0;
            info!(target: DEPOSIT_PHASE, "eth deposit acc balance = {eth_deposit_acc_bal}");

            // validate that the deposit account balance exceeds the eureka routing threshold amount
            if eth_deposit_acc_bal < self.cfg.ethereum.ibc_transfer_threshold_amt {
                // if balance does not exceed the transfer threshold, we skip the eureka transfer steps
                // and proceed to gaia ica -> neutron routing
                info!(target: DEPOSIT_PHASE, "IBC-Eureka transfer threshold not met! Proceeding to ICA routing.");
            } else {
                // if balance meets the transfer threshold, we carry out the eureka transfer steps
                // prior to proceeding to gaia ica -> neutron routing
                info!(target: DEPOSIT_PHASE, "IBC-Eureka transfer threshold met!");

                self.eth_to_gaia_routing(eth_rp, eth_deposit_acc_bal)
                    .await?;
            }
        }

        // Stage 2: deposit token routing from Gaia to Neutron
        {
            let gaia_ica_bal = self
                .gaia_client
                .query_balance(&self.cfg.gaia.ica_address, &self.cfg.gaia.deposit_denom)
                .await?;
            info!(target: DEPOSIT_PHASE, "Cosmos Hub ICA balance = {gaia_ica_bal}");

            // depending on the gaia ICA deposit token balance, we either perform the ICA IBC routing
            // of the balances to Neutron, or proceed to the next stage
            if gaia_ica_bal == 0 {
                info!(target: DEPOSIT_PHASE, "nothing to bridge; proceeding to maxBTC issuance");
            } else {
                info!(target: DEPOSIT_PHASE, "pulling funds to Neutron...");
                self.gaia_to_neutron_routing(gaia_ica_bal).await?;
            }
        }

        // Stage 3: maxBTC issuance
        {
            let neutron_ica_deposit_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.ica_deposit,
                    &self.cfg.neutron.denoms.deposit_token,
                )
                .await?;
            info!(target: DEPOSIT_PHASE, "Neutron ICA deposit account balance = {neutron_ica_deposit_bal}");

            // if there is something in the Neutron ICA deposit account, we trigger the maxBTC issuance
            if neutron_ica_deposit_bal > 0 {
                info!(target: DEPOSIT_PHASE, "Neutron ICA deposit account balance > 0; triggering maxBTC issuance...");
                self.issue_maxbtc().await?;
            } else {
                info!(target: DEPOSIT_PHASE, "Neutron ICA deposit account balance is zero, skipping maxBTC issuance...");
            }
        }

        // Stage 4: Provide liquidity to Supervault
        {
            // Check the balance of the supervault deposit account, which should have received the minted maxBTC
            let supervault_deposit_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.supervault_deposit,
                    &self.cfg.neutron.denoms.maxbtc,
                )
                .await?;
            info!(target: DEPOSIT_PHASE, "Neutron supervault deposit account balance = {supervault_deposit_bal}");

            println!(
                ">>>>>>>>>>>>>>>>>>>>>>>>>>>>>> {} {} ",
                &self.cfg.neutron.accounts.supervault_deposit, &self.cfg.neutron.denoms.maxbtc
            );

            // If there is maxBTC in the supervault deposit account, trigger the liquidity provision
            if supervault_deposit_bal > 0 {
                info!(target: DEPOSIT_PHASE, "Supervault deposit account has maxBTC; triggering LP provision...");
                self.provide_liquidity_to_supervault().await?;
            } else {
                info!(target: DEPOSIT_PHASE, "Supervault deposit account balance is zero, skipping LP provision...");
            }
        }

        Ok(())
    }

    /// performs one action:
    ///   mints maxBTC on Neutron by sending the deposit token from the
    ///   Neutron ICA deposit account to the maxBTC contract. The minted
    ///   maxBTC is sent to the supervault deposit account.
    async fn issue_maxbtc(&mut self) -> anyhow::Result<()> {
        // The `maxbtc_issuer` library is already configured
        // during deployment to send the output to the correct `supervault_deposit` account.
        let maxbtc_issue_msg = valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
            valence_maxbtc_issuer::msg::FunctionMsgs::Issue {},
        );

        // enqueue the function
        info!(target: DEPOSIT_PHASE, "enqueuing maxBTC issue message");
        valence_core::enqueue_neutron(
            &self.neutron_client,
            &self.cfg.neutron.authorizations,
            MAXBTC_ISSUE_LABEL,
            vec![to_json_binary(&maxbtc_issue_msg)?],
        )
        .await?;

        tokio::time::sleep(Duration::from_millis(5000)).await;

        // Before ticking, check the current balance of the target account to ensure polling works
        let pre_issue_supervault_dep_bal = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.supervault_deposit,
                &self.cfg.neutron.denoms.maxbtc,
            )
            .await?;

        info!(target: DEPOSIT_PHASE, "ticking for maxBTC issuance");
        valence_core::tick_neutron(&self.neutron_client, &self.cfg.neutron.processor).await?;

        // Poll until the funds arrive in the supervault deposit account.
        // The exact amount might be hard to predict, so we poll until the balance increases.
        // For simplicity, we assume a direct mapping here, but a real-world scenario might
        // require a more complex check (e.g., querying the contract for expected output).
        info!(target: DEPOSIT_PHASE, "polling for maxBTC to arrive in supervault deposit account...");
        self.neutron_client
            .poll_until_expected_balance(
                &self.cfg.neutron.accounts.supervault_deposit,
                &self.cfg.neutron.denoms.maxbtc,
                pre_issue_supervault_dep_bal,
                5,  // every 5 sec
                30, // for 30 times
            )
            .await?;
        Ok(())
    }

    /// performs one action:
    ///   takes the maxBTC from the supervault deposit account and provides
    ///   it as liquidity to the supervault contract. The resulting LP tokens
    ///   are sent to the final settlement account.
    async fn provide_liquidity_to_supervault(&mut self) -> anyhow::Result<()> {
        let supervault_lp_msg = valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
            valence_supervaults_lper::msg::FunctionMsgs::ProvideLiquidity {
                expected_vault_ratio_range: None,
            }, // TODO?
        );

        info!(target: DEPOSIT_PHASE, "enqueuing supervault LP message");
        valence_core::enqueue_neutron(
            &self.neutron_client,
            &self.cfg.neutron.authorizations,
            PROVIDE_LIQUIDIY_LABEL,
            vec![to_json_binary(&supervault_lp_msg)?],
        )
        .await?;

        // Check the balance of the final settlement account before providing liquidity
        let pre_lp_settlement_bal = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.supervault_lp,
            )
            .await?;

        info!(target: DEPOSIT_PHASE, "ticking for supervault LP provision");
        valence_core::tick_neutron(&self.neutron_client, &self.cfg.neutron.processor).await?;

        // Block execution until the LP tokens arrive in the settlement account.
        // The amount of LP tokens is non-deterministic, so we poll until the balance increases.
        // A more robust solution might query the vault for the expected LP amount out.
        info!(target: DEPOSIT_PHASE, "polling for LP tokens to arrive in settlement account...");
        self.neutron_client
            .poll_until_expected_balance(
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.supervault_lp,
                pre_lp_settlement_bal,
                5,  // every 5 sec
                30, // for 30 times
            )
            .await?;

        Ok(())
    }

    /// This function carries out the steps needed to route the deposits from Ethereum program deposit
    /// account to the configured Cosmos Hub ICA managed by Neutron Valence-ICA.
    async fn eth_to_gaia_routing(
        &mut self,
        eth_rp: &CustomProvider,
        eth_deposit_acc_bal: U256,
    ) -> anyhow::Result<()> {
        let eth_auth_contract = Authorization::new(self.cfg.ethereum.authorizations, &eth_rp);

        // fetch the IBC-Eureka route from eureka client
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

        let post_fee_amount_out_u128 = utils::skip::get_amount_out(&skip_api_response)?;
        info!(target: DEPOSIT_PHASE, "post_fee_amount_out_u128 = {post_fee_amount_out_u128:?}" );

        // format the response in format expected by the coprocessor and post it
        // there for proof
        let coprocessor_input = json!({"skip_response": skip_api_response});

        info!(target: DEPOSIT_PHASE, "co-processor input: {coprocessor_input}");
        info!(
          target: DEPOSIT_PHASE,
          "co-processor ID: {}",
          self.cfg.ethereum.coprocessor_app_ids.ibc_eureka,
        );

        let skip_response_zkp = self
            .coprocessor_client
            .prove(
                &self.cfg.ethereum.coprocessor_app_ids.ibc_eureka,
                &coprocessor_input,
            )
            .await?;

        info!(target: DEPOSIT_PHASE, "co_processor zkp post response: {skip_response_zkp:?}");

        // extract the program and domain parameters by decoding the zkp
        let (proof_program, inputs_program) = utils::decode(skip_response_zkp.program)?;
        let (proof_domain, _) = utils::decode(skip_response_zkp.domain)?;

        // build the eureka transfer zk message from decoded params
        let auth_eureka_transfer_zk_msg = eth_auth_contract.executeZKMessage(
            Bytes::from(inputs_program),
            Bytes::from(proof_program),
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

        // transfer can be considered complete when the current ica balance increases
        // by the expected post_fee ibc eureka transfer amount out
        let pre_routing_gaia_ica_bal = self
            .gaia_client
            .query_balance(&self.cfg.gaia.ica_address, &self.cfg.gaia.deposit_denom)
            .await?;
        let gaia_ica_expected_balance = pre_routing_gaia_ica_bal + post_fee_amount_out_u128;
        info!(
          target: DEPOSIT_PHASE,
          "gaia ica expected bal = {gaia_ica_expected_balance}; polling..."
        );

        // block execution until the funds arrive to the Cosmos Hub ICA owned
        // by the Valence Interchain Account on Neutron.
        // poll for 15sec * 100 = 1500sec = 25min which should suffice for
        // IBC Eureka routing time of 15min
        self.gaia_client
            .poll_until_expected_balance(
                &self.cfg.gaia.ica_address,
                &self.cfg.gaia.deposit_denom,
                gaia_ica_expected_balance,
                15,  // every 15 sec
                100, // for 100 times
            )
            .await?;
        Ok(())
    }

    /// carries out the steps needed to route the deposits from cosmos hub ICA to the
    /// Neutron deposit account.
    async fn gaia_to_neutron_routing(&mut self, gaia_ica_bal: u128) -> anyhow::Result<()> {
        let ica_ibc_transfer_update_msg: valence_library_utils::msg::ExecuteMsg<
            valence_ica_ibc_transfer::msg::FunctionMsgs,
            valence_ica_ibc_transfer::msg::LibraryConfigUpdate,
        > = valence_library_utils::msg::ExecuteMsg::UpdateConfig {
            new_config: valence_ica_ibc_transfer::msg::LibraryConfigUpdate {
                input_addr: None,
                amount: Some(gaia_ica_bal.into()),
                denom: None,
                receiver: None,
                memo: None,
                remote_chain_info: None,
                denom_to_pfm_map: None,
                eureka_config: OptionUpdate::Set(None),
            },
        };
        let ica_ibc_transfer_exec_msg =
            valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
                valence_ica_ibc_transfer::msg::FunctionMsgs::Transfer {},
            );

        info!(target: DEPOSIT_PHASE, "enqueuing ica_ibc_transfer library update & transfer");
        valence_core::enqueue_neutron(
            &self.neutron_client,
            &self.cfg.neutron.authorizations,
            ICA_TRANSFER_LABEL,
            vec![
                to_json_binary(&ica_ibc_transfer_update_msg)?,
                to_json_binary(&ica_ibc_transfer_exec_msg)?,
            ],
        )
        .await?;

        valence_core::ensure_neutron_account_fees_coverage(
            &self.neutron_client,
            &self.cfg.neutron.accounts.gaia_ica,
        )
        .await?;

        // transfer can be considered complete when the current ica balance increases
        // by the expected post_fee ibc eureka transfer amount out
        let pre_routing_neutron_deposit_acc_bal = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.ica_deposit,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;

        let neutron_deposit_acc_expected_bal = pre_routing_neutron_deposit_acc_bal + gaia_ica_bal;
        info!(
          target: DEPOSIT_PHASE,
          "neutron deposit acc expected bal = {neutron_deposit_acc_expected_bal}; polling..."
        );

        info!(target: DEPOSIT_PHASE, "tick: update & transfer");
        valence_core::tick_neutron(&self.neutron_client, &self.cfg.neutron.processor).await?;

        info!(target: DEPOSIT_PHASE, "polling for neutron deposit account to receive the funds");

        // block execution until funds arrive to the Neutron program deposit
        // account
        self.neutron_client
            .poll_until_expected_balance(
                &self.cfg.neutron.accounts.ica_deposit,
                &self.cfg.neutron.denoms.deposit_token,
                neutron_deposit_acc_expected_bal,
                5,
                30,
            )
            .await?;

        Ok(())
    }
}
