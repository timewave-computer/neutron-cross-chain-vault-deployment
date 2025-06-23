use std::error::Error;

use alloy::primitives::U256;
use cosmwasm_std::Binary;
use log::info;
use packages::labels::REGISTER_OBLIGATION_LABEL;
use serde_json::json;
use valence_domain_clients::{
    coprocessor::base_client::CoprocessorBaseClient,
    cosmos::{base_client::BaseClient, wasm_client::WasmClient},
    indexer::one_way_vault::OneWayVaultIndexer,
};

use crate::strategy_config::Strategy;

const REGISTRATION_PHASE: &str = "registration";

impl Strategy {
    /// reads the newly submitted withdrawal obligations that are not yet
    /// present in the Clearing Queue, generates their zero-knowledge proofs,
    /// and posts them into the Clearing queue in order.
    /// consists of the following stages:
    /// 1. fetching all new withdraw obligations from the indexer
    /// 2. generating ZKP for each of the newly fetched obligations
    /// 3. posting ZKPs to the neutron authorizations module before
    ///    attempting to enqueue them
    pub async fn register_withdraw_obligations(
        &mut self,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!(target: REGISTRATION_PHASE, "starting withdraw obligation registration phase");

        // query the Clearing Queue library for the latest posted withdraw request ID
        let clearing_queue_cfg: valence_clearing_queue_supervaults::msg::Config = self
            .neutron_client
            .query_contract_state(
                &self.cfg.neutron.libraries.clearing_queue,
                valence_clearing_queue_supervaults::msg::QueryMsg::GetLibraryConfig {},
            )
            .await?;

        // get id of the latest obligation request that was registered on neutron
        let latest_registered_obligation_id =
            clearing_queue_cfg.latest_id.unwrap_or_default().u64();
        info!(
            target: REGISTRATION_PHASE,
            "latest_registered_obligation_id={latest_registered_obligation_id}"
        );

        // query the OneWayVault indexer to fetch all obligations that were registered
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

        // process the new OneWayVault Withdraw events in order from the oldest
        // to the newest, posting them to the coprocessor to obtain a ZKP
        for (obligation_id, ..) in new_obligations {
            info!(target: REGISTRATION_PHASE, "processing obligation_id={obligation_id}");

            // build the json input for coprocessor client
            let withdraw_id_json = json!({"withdraw_request_id": obligation_id});

            // post the proof request to the coprocessor client & await
            info!(target: REGISTRATION_PHASE, "posting proof request to coprocessor client: {withdraw_id_json}");
            let vault_zkp_response = self
                .coprocessor_client
                .prove(&self.cfg.coprocessor.vault_circuit_id, &withdraw_id_json)
                .await?;

            // extract the program and domain parameters by decoding the zkp
            let (proof_program, inputs_program) = vault_zkp_response.program.decode()?;
            let (proof_domain, inputs_domain) = vault_zkp_response.domain.decode()?;

            // submits the decoded zkp parameters to the program authorizations module
            self.post_zkp_on_chain(
                (proof_program, inputs_program),
                (proof_domain, inputs_domain),
            )
            .await?;

            // tick the processor to register the obligation to the clearing queue
            self.tick_neutron().await?;
        }

        info!(target: REGISTRATION_PHASE, "finished processing withdraw requests; concluding obligation registration phase...");

        Ok(())
    }

    /// constructs the zk authorization execution message and executes it.
    /// authorizations module will perform the zk verification and, if
    /// successful, push it to the processor for execution
    async fn post_zkp_on_chain(
        &mut self,
        (proof_program, inputs_program): (Vec<u8>, Vec<u8>),
        (proof_domain, inputs_domain): (Vec<u8>, Vec<u8>),
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // construct the zk authorization registration message
        let execute_zk_authorization_msg =
            valence_authorization_utils::msg::PermissionlessMsg::ExecuteZkAuthorization {
                label: REGISTER_OBLIGATION_LABEL.to_string(),
                message: Binary::from(inputs_program),
                proof: Binary::from(proof_program),
                domain_message: Binary::from(inputs_domain),
                domain_proof: Binary::from(proof_domain),
            };

        // execute the zk authorization. this will perform the verification
        // and, if successful, push the msg to the processor
        info!(target: REGISTRATION_PHASE, "executing zk authorization");

        let tx_resp = self
            .neutron_client
            .execute_wasm(
                &self.cfg.neutron.authorizations,
                valence_authorization_utils::msg::ExecuteMsg::PermissionlessAction(
                    execute_zk_authorization_msg,
                ),
                vec![],
                None,
            )
            .await?;

        // poll for inclusion to avoid account sequence mismatch errors
        self.neutron_client.poll_for_tx(&tx_resp.hash).await?;

        Ok(())
    }
}
