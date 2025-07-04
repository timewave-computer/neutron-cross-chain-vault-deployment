use cosmwasm_std::Binary;

use log::{debug, info};
use valence_authorization_utils::msg::ProcessorMessage;
use valence_domain_clients::{
    clients::neutron::NeutronClient,
    cosmos::{base_client::BaseClient, wasm_client::WasmClient},
};

use crate::{
    labels::REGISTER_OBLIGATION_LABEL,
    phases::{DEPOSIT_PHASE, REGISTRATION_PHASE},
};

const ICA_CONTRACT_FUNDING_AMT: u128 = 200_000;

pub async fn enqueue_neutron(
    client: &NeutronClient,
    authorizations: &str,
    label: &str,
    messages: Vec<Binary>,
) -> anyhow::Result<()> {
    let mut encoded_messages = vec![];

    for message in messages {
        let processor_msg = ProcessorMessage::CosmwasmExecuteMsg { msg: message };

        encoded_messages.push(processor_msg);
    }

    let tx_resp = client
        .execute_wasm(
            authorizations,
            valence_authorization_utils::msg::ExecuteMsg::PermissionlessAction(
                valence_authorization_utils::msg::PermissionlessMsg::SendMsgs {
                    label: label.to_string(),
                    messages: encoded_messages,
                    ttl: None,
                },
            ),
            vec![],
            None,
        )
        .await?;

    debug!("tx hash: {}", tx_resp.hash);

    client.poll_for_tx(&tx_resp.hash).await?;

    Ok(())
}

/// ticks the processor on neutron
pub async fn tick_neutron(client: &NeutronClient, processor: &str) -> anyhow::Result<()> {
    let tx_resp = client
        .execute_wasm(
            processor,
            valence_processor_utils::msg::ExecuteMsg::PermissionlessAction(
                valence_processor_utils::msg::PermissionlessMsg::Tick {},
            ),
            vec![],
            None,
        )
        .await?;

    debug!("tx hash: {}", tx_resp.hash);

    client.poll_for_tx(&tx_resp.hash).await?;

    Ok(())
}

/// constructs the zk authorization execution message and executes it.
/// authorizations module will perform the zk verification and, if
/// successful, push it to the processor for execution
pub async fn post_zkp_on_chain(
    client: &NeutronClient,
    authorizations: &str,
    (proof_program, inputs_program): (Vec<u8>, Vec<u8>),
    (proof_domain, inputs_domain): (Vec<u8>, Vec<u8>),
) -> anyhow::Result<()> {
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

    let tx_resp = client
        .execute_wasm(
            authorizations,
            valence_authorization_utils::msg::ExecuteMsg::PermissionlessAction(
                execute_zk_authorization_msg,
            ),
            vec![],
            None,
        )
        .await?;

    // poll for inclusion to avoid account sequence mismatch errors
    client.poll_for_tx(&tx_resp.hash).await?;

    Ok(())
}

pub async fn ensure_neutron_account_fees_coverage(
    client: &NeutronClient,
    acc: &str,
) -> anyhow::Result<()> {
    let account_ntrn_balance = client.query_balance(acc, "untrn").await?;

    if account_ntrn_balance < ICA_CONTRACT_FUNDING_AMT {
        let delta = ICA_CONTRACT_FUNDING_AMT - account_ntrn_balance;

        info!(target: DEPOSIT_PHASE, "Funding neutron account with {delta}untrn for ibc tx fees...");
        let transfer_rx = client.transfer(acc, delta, "untrn", None).await?;
        client.poll_for_tx(&transfer_rx.hash).await?;
    }

    Ok(())
}
