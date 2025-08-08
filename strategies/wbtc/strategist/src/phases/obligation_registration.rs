use alloy::primitives::U256;
use cosmwasm_std::Uint64;
use log::info;
use packages::{
    phases::REGISTRATION_PHASE,
    utils::{self, valence_core},
};
use serde_json::json;
use valence_domain_clients::{
    coprocessor::base_client::CoprocessorBaseClient, cosmos::wasm_client::WasmClient,
    indexer::one_way_vault::OneWayVaultIndexer,
};

use crate::strategy_config::Strategy;

impl Strategy {
    /// reads the newly submitted withdrawal obligations that are not yet
    /// present in the Clearing Queue, generates their zero-knowledge proofs,
    /// and posts them into the Clearing queue in order.
    /// consists of the following stages:
    /// 1. fetching all new withdraw obligations from the indexer
    /// 2. generating ZKP for each of the newly fetched obligations
    /// 3. posting ZKPs to the neutron authorizations module before
    ///    attempting to enqueue them
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

        // get id of the latest obligation request that was registered on neutron
        let latest_registered_obligation_id = match clearing_queue_cfg.latest_id {
            // if there is a latest id, we increment it by 1 to only fetch the
            // new withdraw requests
            Some(id) => Some(id.checked_add(Uint64::one())?.u64()),
            // if there is no latest id, we return the same to fetch everything
            None => None,
        };

        // query the OneWayVault indexer to fetch all obligations that were registered
        // on the vault but are not yet registered into the queue on Neutron
        let new_obligations: Vec<(u64, alloy::primitives::Address, String, U256)> = self
            .indexer_client
            .query_vault_withdraw_requests(latest_registered_obligation_id, true)
            .await
            .unwrap_or_default();
        info!(
            target: REGISTRATION_PHASE,
            "new_obligations = {new_obligations:#?}"
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
                .prove(&self.cfg.neutron.coprocessor_app_ids.clearing_queue, &withdraw_id_json)
                .await?;

            // extract the program and domain parameters by decoding the zkp
            let (proof_program, inputs_program) = utils::decode(vault_zkp_response.program)?;
            let (proof_domain, inputs_domain) = utils::decode(vault_zkp_response.domain)?;

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
