use alloy::{
    primitives::{Bytes, U256},
    providers::Provider,
};

use crate::strategy_config::Strategy;
use cosmwasm_std::to_json_binary;
use log::{info, warn};
use packages::{
    labels::{ICA_TRANSFER_LABEL, MAXBTC_ISSUE_LABEL},
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
    /// Neutron (via Cosmos Hub) before minting the maxBTC.
    /// consists of three stages:
    /// 1. Ethereum -> Hub routing
    /// 2. Hub -> Neutron routing
    /// 3. maxBTC issuing
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

            // validate that the deposit account balance exceeds the eureka routing
            // threshold amount
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
                info!(target: DEPOSIT_PHASE, "nothing to bridge; proceeding to position entry");
            } else {
                info!(target: DEPOSIT_PHASE, "pulling funds to Neutron...");
                self.gaia_to_neutron_routing(gaia_ica_bal).await?;
            }
        }

        // Stage 3: maxBTC issuance
        {
            let neutron_deposit_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.ica_deposit,
                    &self.cfg.neutron.denoms.deposit_token,
                )
                .await?;
            info!(target: DEPOSIT_PHASE, "Neutron deposit account balance = {neutron_deposit_bal}");

            // if there is something in the Neutron deposit account, we trigger the maxBTC issuance
            if neutron_deposit_bal > 0 {
                info!(target: DEPOSIT_PHASE, "Neutron deposit account balance = {neutron_deposit_bal}; triggering maxBTC issuance...");
                self.issue_maxbtc().await?;
            } else {
                info!(target: DEPOSIT_PHASE, "Neutron deposit account balance is zero, skipping...");
            }
        }

        // TODO: change permissions in neutron_initialization.rs

        // TODO: change:
        /* // Instantiate the maxBTC issuer library
        let maxbtc_issuer_config = valence_maxbtc_issuer::msg::LibraryConfig {
            input_addr: LibraryAccountType::Addr(predicted_base_accounts[0].clone()),
            output_addr: LibraryAccountType::Addr(predicted_base_accounts[1].clone()), <<<<<<<<
            maxbtc_issuer_addr: params.program.maxbtc_contract.clone(),
            btc_denom: params.program.deposit_token_on_neutron_denom.clone(),
        };
        */

        // TODO: update clearing queue

        // Stage 3: Mars & Supervault position entry on Neutron
        // {
        //     let neutron_deposit_bal = self
        //         .neutron_client
        //         .query_balance(
        //             &self.cfg.neutron.accounts.deposit,
        //             &self.cfg.neutron.denoms.deposit_token,
        //         )
        //         .await?;
        //
        //     // depending on the neutron deposit account balance, we either conclude the deposit phase
        //     // or perform the configured split before entering into Mars and Supervault positions.
        //     match neutron_deposit_bal < MIN_SPLIT_BALANCE {
        //         true => {
        //             info!(target: DEPOSIT_PHASE, "Neutron deposit account balance is insufficient for entry! concluding the deposit phase...");
        //         }
        //         false => {
        //             info!(target: DEPOSIT_PHASE, "Neutron deposit account balance = {neutron_deposit_bal}; lending & LPing...");
        //             // use Supervaults lper library to deposit funds from Supervaults deposit account
        //             // into the configured supervault
        //             let supervaults_lper_execute_msg =
        //                 valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
        //                     valence_supervaults_lper::msg::FunctionMsgs::ProvideLiquidity {
        //                         expected_vault_ratio_range: None,
        //                     },
        //                 );
        //             let supervaults_lper_execute_msg =
        //                 to_json_binary(&supervaults_lper_execute_msg)?;
        //
        //             // enqueue all three actions under a single label as its an atomic subroutine
        //             valence_core::enqueue_neutron(
        //                 &self.neutron_client,
        //                 &self.cfg.neutron.authorizations,
        //                 LEND_AND_PROVIDE_LIQUIDITY_LABEL,
        //                 vec![
        //                     supervaults_lper_execute_msg.clone(),
        //                     supervaults_lper_execute_msg,
        //                 ],
        //             )
        //             .await?;
        //
        //             valence_core::tick_neutron(&self.neutron_client, &self.cfg.neutron.processor)
        //                 .await?;
        //         }
        //     }
        // }

        Ok(())
    }

    /// performs one action:
    ///    mints maxBTC on Neutron by sending the deposit token from the
    ///    Neutron deposit account to the maxBTC contract
    async fn issue_maxbtc(&mut self) -> anyhow::Result<()> {
        // use Splitter to route funds from the Neutron program deposit
        // account to the Mars and Supervaults deposit accounts
        let maxbtc_issue_msg = valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
            valence_maxbtc_issuer::msg::FunctionMsgs::Issue {},
        );

        // enqueue the function
        valence_core::enqueue_neutron(
            &self.neutron_client,
            &self.cfg.neutron.authorizations,
            MAXBTC_ISSUE_LABEL,
            vec![to_json_binary(&maxbtc_issue_msg)?],
        )
        .await?;

        valence_core::tick_neutron(&self.neutron_client, &self.cfg.neutron.processor).await?;

        Ok(())
    }

    /// carries out the steps needed to route the deposits from Ethereum program deposit
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
        let (proof_domain, inputs_domain) = utils::decode(skip_response_zkp.domain)?;

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
    /// two messages are enqueued:
    /// 1. update the ica ibc transfer library transfer amount
    /// 2. trigger the ica ibc transfer
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
