use std::{error::Error, str::FromStr};

use alloy::{primitives::Bytes, providers::Provider};
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
    /// carries out the steps needed to bring the new deposits from Ethereum to
    /// Neutron (via Cosmos Hub) before depositing them into Mars protocol.
    pub async fn deposit(
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
        // TODO: this is too naive; only need to skip the Eureka routing eth->gaia
        // in case this threshold is not exceeded so that any funds that landed in the ICA
        // later than expected would still get pulled into the positions
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

        let gaia_ica_bal = self
            .gaia_client
            .poll_until_expected_balance(
                &self.cfg.gaia.ica_address,
                &self.cfg.gaia.deposit_denom,
                gaia_ica_balance.u128(),
                5,
                30,
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
}
