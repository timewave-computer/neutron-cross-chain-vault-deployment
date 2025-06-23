use std::error::Error;

use cosmwasm_std::Binary;

use log::debug;
use valence_authorization_utils::msg::ProcessorMessage;
use valence_domain_clients::{
    clients::neutron::NeutronClient,
    cosmos::{base_client::BaseClient, wasm_client::WasmClient},
};

pub async fn enqueue_neutron(
    client: &NeutronClient,
    authorizations: &str,
    label: &str,
    messages: Vec<Binary>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
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
pub async fn tick_neutron(
    client: &NeutronClient,
    processor: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
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
