use std::error::Error;

use alloy::primitives::U256;
use log::{info, trace};
use serde_json::json;
use types::labels::REGISTER_OBLIGATION_LABEL;
use valence_domain_clients::{
    coprocessor::base_client::CoprocessorBaseClient, cosmos::wasm_client::WasmClient,
    indexer::one_way_vault::OneWayVaultIndexer,
};

use crate::strategy_config::Strategy;

const REGISTRATION_PHASE: &str = "registration";

impl Strategy {
    /// reads the newly submitted withdrawal obligations that are not yet
    /// present in the Clearing Queue, generates their zero-knowledge proofs,
    /// and posts them into the Clearing queue in order.
    pub async fn register_withdraw_obligations(
        &mut self,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
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
}
