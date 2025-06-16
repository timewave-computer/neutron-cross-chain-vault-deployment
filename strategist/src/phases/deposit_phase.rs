use std::{error::Error, str::FromStr};

use alloy::{
    primitives::{Bytes, U256},
    providers::Provider,
};
use cosmwasm_std::{Uint128, to_json_binary};
use log::{info, trace, warn};
use serde_json::json;
use types::{
    labels::{ICA_TRANSFER_LABEL, LEND_AND_PROVIDE_LIQUIDITY_LABEL},
    sol_types::{Authorization, BaseAccount, ERC20},
};
use valence_domain_clients::{
    coprocessor::base_client::CoprocessorBaseClient,
    cosmos::base_client::BaseClient,
    evm::base_client::{CustomProvider, EvmBaseClient},
};
use valence_library_utils::OptionUpdate;

use crate::strategy_config::Strategy;

const DEPOSIT_PHASE: &str = "deposit";

impl Strategy {
    async fn eth_to_gaia_routing(
        &mut self,
        eth_rp: &CustomProvider,
        eth_deposit_acc_bal: U256,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let eth_auth_contract = Authorization::new(self.cfg.ethereum.authorizations, &eth_rp);

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
        info!(target: DEPOSIT_PHASE, "posting skip-api response to co-processor app id: {}", &self.cfg.coprocessor.eureka_circuit_id);
        let skip_response_zkp = self
            .coprocessor_client
            .prove(&self.cfg.coprocessor.eureka_circuit_id, &coprocessor_input)
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

        // 4. block execution until the funds arrive to the Cosmos Hub ICA owned
        // by the Valence Interchain Account on Neutron
        // TODO: doublecheck the precision conversion here
        let gaia_ica_balance = Uint128::from_str(&eth_deposit_acc_bal.to_string())?;
        info!(target: DEPOSIT_PHASE, "gaia ica expected deposit token bal = {gaia_ica_balance}; starting to poll");

        self.gaia_client
            .poll_until_expected_balance(
                &self.cfg.gaia.ica_address,
                &self.cfg.gaia.deposit_denom,
                gaia_ica_balance.u128(),
                5,
                30,
            )
            .await?;
        Ok(())
    }

    async fn gaia_to_neutron_routing(
        &mut self,
        gaia_ica_bal: u128,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // TODO: this should look like this but there's some serde issue so using manual
        // json below for now

        // let ica_ibc_transfer_update_msg: valence_library_utils::msg::ExecuteMsg<
        //     valence_ica_ibc_transfer::msg::FunctionMsgs,
        //     valence_ica_ibc_transfer::msg::LibraryConfigUpdate,
        // > = valence_library_utils::msg::ExecuteMsg::UpdateConfig {
        //     new_config: valence_ica_ibc_transfer::msg::LibraryConfigUpdate {
        //         input_addr: None,
        //         amount: Some(gaia_ica_bal.into()),
        //         denom: None,
        //         receiver: None,
        //         memo: None,
        //         remote_chain_info: None,
        //         denom_to_pfm_map: None,
        //         eureka_config: OptionUpdate::None,
        //     },
        // };
        let ica_ibc_transfer_update_msg = json!({
            "update_config": {
                "new_config": {
                    "amount": "11099"
                }
            }
        });
        let ica_ibc_transfer_update_msg = to_json_binary(&ica_ibc_transfer_update_msg)?;

        let ica_ibc_transfer_exec_msg: valence_library_utils::msg::ExecuteMsg<
            valence_ica_ibc_transfer::msg::FunctionMsgs,
            valence_ica_ibc_transfer::msg::LibraryConfigUpdate,
        > = valence_library_utils::msg::ExecuteMsg::ProcessFunction(
            valence_ica_ibc_transfer::msg::FunctionMsgs::Transfer {},
        );

        let ica_ibc_transfer_msg = to_json_binary(&ica_ibc_transfer_exec_msg)?;

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
                gaia_ica_bal,
                5,
                10,
            )
            .await?;
        Ok(())
    }

    /// carries out the steps needed to bring the new deposits from Ethereum to
    /// Neutron (via Cosmos Hub) before depositing them into Mars protocol.
    pub async fn deposit(
        &mut self,
        eth_rp: &CustomProvider,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        trace!(target: DEPOSIT_PHASE, "starting deposit phase");

        let eth_wbtc_contract = ERC20::new(self.cfg.ethereum.denoms.deposit_token, &eth_rp);
        let eth_deposit_acc = BaseAccount::new(self.cfg.ethereum.accounts.deposit, &eth_rp);

        // 1. query the ethereum deposit account balance
        let eth_deposit_acc_bal = self
            .eth_client
            .query(eth_wbtc_contract.balanceOf(*eth_deposit_acc.address()))
            .await?
            ._0;
        info!(target: DEPOSIT_PHASE, "eth deposit acc balance = {eth_deposit_acc_bal}");

        // 2. validate that the deposit account balance exceeds the eureka routing
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

        let gaia_ica_bal = self
            .gaia_client
            .query_balance(&self.cfg.gaia.ica_address, &self.cfg.gaia.deposit_denom)
            .await?;

        match gaia_ica_bal == 0 {
            true => {
                info!(target: DEPOSIT_PHASE, "Cosmos Hub ICA balance is zero! proceeding to position entry");
            }
            false => {
                info!(target: DEPOSIT_PHASE, "Cosmos Hub ICA deposit token balance is {gaia_ica_bal}; pulling funds to Neutron");
                self.gaia_to_neutron_routing(gaia_ica_bal).await?;
            }
        }

        // 5. enqueue: gaia ICA transfer amount update & gaia ica transfer messages

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
}
