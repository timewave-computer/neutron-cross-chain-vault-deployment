use std::{
    error::Error,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use alloy::{
    primitives::{Bytes, U256},
    providers::Provider,
};
use cosmwasm_std::to_json_binary;
use log::{info, trace, warn};
use packages::{
    labels::{ICA_TRANSFER_LABEL, LEND_AND_PROVIDE_LIQUIDITY_LABEL},
    phases::DEPOSIT_PHASE,
    types::sol_types::{Authorization, BaseAccount, ERC20},
    utils::valence_core,
};
use serde_json::json;
use valence_domain_clients::{
    coprocessor::base_client::CoprocessorBaseClient,
    cosmos::base_client::BaseClient,
    evm::base_client::{CustomProvider, EvmBaseClient},
};

use crate::strategy_config::Strategy;

/// minimum Valence account balance to perform a split
const MIN_SPLIT_BALANCE: u128 = 2;

impl Strategy {
    /// carries out the steps needed to bring the new deposits from Ethereum to
    /// Neutron (via Lombard & Cosmos Hub) before depositing them into Mars protocol.
    /// consists of three stages:
    /// 1. Ethereum -> Hub routing
    /// 2. Hub -> Neutron routing
    /// 3. Supervaults & Mars position entry
    pub async fn deposit(
        &mut self,
        eth_rp: &CustomProvider,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        trace!(target: DEPOSIT_PHASE, "starting deposit phase");

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
            match eth_deposit_acc_bal < self.cfg.ethereum.ibc_transfer_threshold_amt {
                // if balance does not exceed the transfer threshold, we skip the eureka transfer steps
                // and proceed to gaia ica -> neutron routing
                true => {
                    info!(target: DEPOSIT_PHASE, "IBC-Eureka transfer threshold not met! Proceeding to ICA routing.");
                }
                // if balance meets the transfer threshold, we carry out the eureka transfer steps
                // prior to proceeding to gaia ica -> neutron routing
                false => {
                    info!(target: DEPOSIT_PHASE, "IBC-Eureka transfer threshold met!");
                    self.eth_to_gaia_routing(eth_rp, eth_deposit_acc_bal)
                        .await?;
                }
            }
        }

        // Stage 2: deposit token routing from Gaia to Neutron
        {
            let gaia_ica_bal = self
                .gaia_client
                .query_balance(&self.cfg.gaia.ica_address, &self.cfg.gaia.deposit_denom)
                .await?;

            // depending on the gaia ICA deposit token balance, we either perform the ICA IBC routing
            // of the balances to Neutron, or proceed to the next stage
            match gaia_ica_bal == 0 {
                true => {
                    info!(target: DEPOSIT_PHASE, "Cosmos Hub ICA balance is zero! proceeding to position entry");
                }
                false => {
                    info!(target: DEPOSIT_PHASE, "Cosmos Hub ICA deposit token balance is {gaia_ica_bal}; pulling funds to Neutron");
                    self.gaia_to_neutron_routing(gaia_ica_bal).await?;
                }
            }
        }

        // Stage 3: Mars & Supervault position entry on Neutron
        {
            let neutron_deposit_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.deposit,
                    &self.cfg.neutron.denoms.deposit_token,
                )
                .await?;

            // depending on the neutron deposit account balance, we either conclude the deposit phase
            // or perform the configured split before entering into Mars and Supervault positions.
            match neutron_deposit_bal < MIN_SPLIT_BALANCE {
                true => {
                    info!(target: DEPOSIT_PHASE, "Neutron deposit account balance is insufficient for entry! concluding the deposit phase...");
                }
                false => {
                    info!(target: DEPOSIT_PHASE, "Neutron deposit account balance = {neutron_deposit_bal}; lending & LPing...");
                    // use Splitter to route funds from the Neutron program deposit
                    // account to the Mars and Supervaults deposit accounts
                    let splitter_exec_msg =
                        valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
                            valence_splitter_library::msg::FunctionMsgs::Split {},
                        );

                    // use Mars Lending library to lend funds from Mars deposit account
                    // into Mars protocol
                    let mars_lending_exec_msg =
                        valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
                            valence_mars_lending::msg::FunctionMsgs::Lend {},
                        );

                    // use Supervaults lper library to deposit funds from Supervaults deposit account
                    // into the configured supervault
                    let supervaults_lper_execute_msg =
                        valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
                            valence_supervaults_lper::msg::FunctionMsgs::ProvideLiquidity {
                                expected_vault_ratio_range: None,
                            },
                        );

                    // enqueue all three actions under a single label as its an atomic subroutine
                    valence_core::enqueue_neutron(
                        &self.neutron_client,
                        &self.cfg.neutron.authorizations,
                        LEND_AND_PROVIDE_LIQUIDITY_LABEL,
                        vec![
                            to_json_binary(&splitter_exec_msg)?,
                            to_json_binary(&mars_lending_exec_msg)?,
                            to_json_binary(&supervaults_lper_execute_msg)?,
                        ],
                    )
                    .await?;

                    valence_core::tick_neutron(&self.neutron_client, &self.cfg.neutron.processor)
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// carries out the steps needed to route the deposits from Ethereum program deposit
    /// account to the configured Lombard ICA managed by Neutron Valence-ICA.
    async fn eth_to_gaia_routing(
        &mut self,
        eth_rp: &CustomProvider,
        eth_deposit_acc_bal: U256,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let eth_auth_contract = Authorization::new(self.cfg.ethereum.authorizations, &eth_rp);

        // fetch the IBC-Eureka route from eureka client
        let mut skip_api_response = match self
            .ibc_eureka_client
            .query_skip_eureka_route(eth_deposit_acc_bal.to_string())
            .await
        {
            // TODO: consider more thorough checks on this response as even in case of an error this
            // may return something seemingly legit (json-value wise)
            Ok(r) => r,
            Err(e) => {
                warn!(target: DEPOSIT_PHASE, "skip route error: {e}");
                return Ok(());
            }
        };

        let post_fee_amount_out = skip_api_response
            .get("amount_out")
            .cloned()
            .expect("failed to get amount_out from skip api response");

        // 12 hours in secs, the timeout being used in skip ui
        let timeout_duration = Duration::from_secs(12 * 60 * 60);
        let timeout_time = SystemTime::now()
            .checked_add(timeout_duration)
            .ok_or("failed to extend current time by 12h")?;

        let timeout_timestamp_nanos = timeout_time
            .duration_since(UNIX_EPOCH)
            .expect("bad times")
            .as_nanos();

        let skip_response_operations = skip_api_response
            .get("operations")
            .cloned()
            .ok_or("failed to get operations")?
            .as_array()
            .cloned()
            .ok_or("operations not an array")?;
        let skip_response_eureka_operation = skip_response_operations
            .iter()
            .find(|op| op.get("eureka_transfer").cloned().is_some())
            .ok_or("no eureka transfer operation in skip response")?;

        // current circuit expects array with a single element so we override
        // the existing array
        skip_api_response["operations"] = json!([skip_response_eureka_operation]);

        let temp_memo = json!(
            {
              "dest_callback": {
                "address": self.cfg.lombard.callback_contract
              },
              "wasm": {
                "contract": self.cfg.lombard.entry_contract,
                "msg": {
                  "swap_and_action": {
                    "user_swap": {
                      "swap_exact_asset_in": {
                        "swap_venue_name": "ledger-lbtc-convert",
                        "operations": [
                          {
                            "pool": "",
                            "denom_in": self.cfg.lombard.eureka_denom,
                            "denom_out": self.cfg.lombard.native_denom
                          }
                        ]
                      }
                    },
                    "min_asset": {
                      "native": {
                        "denom": self.cfg.lombard.native_denom,
                        "amount": post_fee_amount_out
                      }
                    },
                    "timeout_timestamp": timeout_timestamp_nanos,
                    "post_swap_action": {
                      "ibc_transfer": {
                        "ibc_info": {
                          "source_channel": "channel-0",
                          "receiver": self.cfg.gaia.ica_address,
                          "memo": "",
                          "recover_address": self.cfg.lombard.ica
                        }
                      }
                    },
                    "affiliates": []
                  }
                }
              }
            }
        );

        // format the response in format expected by the coprocessor and post it
        // there for proof
        let coprocessor_input = json!({"skip_response": skip_api_response, "memo": temp_memo});

        info!(
            target: DEPOSIT_PHASE,
            "co-processor input: {}",
            coprocessor_input,
        );

        let skip_response_zkp = self
            .coprocessor_client
            .prove(
                &self.cfg.ethereum.coprocessor_app_ids.ibc_eureka,
                &coprocessor_input,
            )
            .await?;

        info!(target: DEPOSIT_PHASE, "co_processor zkp post response: {:?}", skip_response_zkp);

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

        // block execution until the funds arrive to the Cosmos Hub ICA owned
        // by the Valence Interchain Account on Neutron.
        // we poll
        let post_fee_amount_out_u128: u128 = post_fee_amount_out
            .to_string()
            .parse()
            .expect("Failed to parse u128");
        info!(target: DEPOSIT_PHASE, "gaia ica expected deposit token bal = {post_fee_amount_out_u128}; starting to poll");

        // poll for 15sec * 100 = 1500sec = 25min which should suffice for
        // IBC Eureka routing time of 15min
        self.gaia_client
            .poll_until_expected_balance(
                &self.cfg.gaia.ica_address,
                &self.cfg.gaia.deposit_denom,
                // TODO: think about querying the current ica balance and adding
                // this amount on top
                post_fee_amount_out_u128,
                15,  // every 15 sec
                100, // for 100 times
            )
            .await?;
        Ok(())
    }

    async fn gaia_to_neutron_routing(
        &mut self,
        gaia_ica_bal: u128,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let ica_ibc_transfer_update_msg = json!({
            "update_config": {
                "new_config": {
                    "amount": gaia_ica_bal.to_string(),
                    "eureka_config": "none"
                }
            }
        });
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

        info!(target: DEPOSIT_PHASE, "tick: update & transfer");
        valence_core::tick_neutron(&self.neutron_client, &self.cfg.neutron.processor).await?;

        info!(target: DEPOSIT_PHASE, "polling for neutron deposit account to receive the funds");

        // block execution until funds arrive to the Neutron program deposit
        // account
        self.neutron_client
            .poll_until_expected_balance(
                &self.cfg.neutron.accounts.deposit,
                &self.cfg.neutron.denoms.deposit_token,
                gaia_ica_bal,
                5,
                30,
            )
            .await?;

        Ok(())
    }
}
