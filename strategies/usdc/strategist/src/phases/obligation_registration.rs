use log::info;
use packages::{phases::REGISTRATION_PHASE, utils::valence_core};
use serde_json::json;
use valence_domain_clients::{
    coprocessor::base_client::CoprocessorBaseClient, cosmos::wasm_client::WasmClient,
    indexer::one_way_vault::OneWayVaultIndexer,
};

use crate::strategy_config::Strategy;

impl Strategy {
    pub async fn register_withdraw_obligations(&mut self) -> anyhow::Result<()> {
        info!(target: REGISTRATION_PHASE, "starting withdraw obligation registration phase");

        // query the Clearing Queue library for the latest posted withdraw request ID
        let clearing_queue_cfg: valence_clearing_queue_supervaults::msg::Config = self
            .neutron_client
            .query_contract_state(
                &self.cfg.neutron.libraries.clearing_queue,
                valence_clearing_queue_supervaults::msg::QueryMsg::GetLibraryConfig {},
            )
            .await?;

        info!(
            target: REGISTRATION_PHASE,
            "latest_registered_obligation_id={:?}", clearing_queue_cfg.latest_id
        );

        // prepare the start_id for the indexer query. if there is no latest_id on the
        // clearing queue config, meaning there are no obligations registered yet, we
        // default to 0 to fetch everything. otherwise we increment the id by 1 to only
        // fetch the new withdraw requests that have not been posted to the clearing
        // queue yet.
        let start_id: u64 = clearing_queue_cfg
            .latest_id
            .map_or(0, |id| id.u64().saturating_add(1));

        // query the OneWayVault indexer to fetch all obligations that were registered
        // on the vault but are not yet registered into the queue on Neutron
        let new_obligations = self
            .indexer_client
            .query_vault_withdraw_requests(Some(start_id), true)
            .await?;

        if new_obligations.is_empty() {
            info!(target: REGISTRATION_PHASE, "no new withdraw requests; concluding obligation registration phase...");
            return Ok(());
        }
        info!(target: REGISTRATION_PHASE, "new_obligations = {:#?}", new_obligations);

        // process the new OneWayVault Withdraw events in order from the oldest
        // to the newest, posting them to the coprocessor to obtain a ZKP
        for (obligation_id, ..) in new_obligations {
            info!(target: REGISTRATION_PHASE, "processing obligation #{obligation_id}");

            // build the json input for coprocessor client
            let withdraw_id_json = json!({"withdraw_request_id": obligation_id});

            // post the proof request to the coprocessor client & await
            info!(target: REGISTRATION_PHASE, "posting proof request to coprocessor client: {withdraw_id_json}");
            let vault_zkp_response = self
                .coprocessor_client
                .prove(
                    &self.cfg.neutron.coprocessor_app_ids.clearing_queue,
                    &withdraw_id_json,
                )
                .await?;
            info!(target: REGISTRATION_PHASE, "vault zkp resp: {vault_zkp_response:?}");

            // extract the program and domain parameters by decoding the zkp
            let (proof_program, inputs_program) = vault_zkp_response.program.decode()?;
            let (proof_domain, inputs_domain) = vault_zkp_response.domain.decode()?;

            // submits the decoded zkp parameters to the program authorizations module
            valence_core::post_zkp_on_chain(
                &self.neutron_client,
                &self.cfg.neutron.authorizations,
                (proof_program, inputs_program),
                (proof_domain, inputs_domain),
            )
            .await?;

            // tick the processor to register the obligation to the clearing queue
            valence_core::tick_neutron(&self.neutron_client, &self.cfg.neutron.processor).await?;
        }

        info!(target: REGISTRATION_PHASE, "finished processing withdraw requests; concluding obligation registration phase...");

        Ok(())
    }
}
