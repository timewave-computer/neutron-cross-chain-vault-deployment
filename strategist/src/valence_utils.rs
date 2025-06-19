pub(crate) mod neutron {
    use std::error::Error;

    use cosmwasm_std::Binary;

    use log::{debug, info};
    use valence_authorization_utils::msg::ProcessorMessage;
    use valence_domain_clients::cosmos::{base_client::BaseClient, wasm_client::WasmClient};

    use crate::strategy_config::Strategy;

    impl Strategy {
        /// enqueues a message on neutron
        pub async fn enqueue_neutron(
            &mut self,
            label: &str,
            messages: Vec<Binary>,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            let mut encoded_messages = vec![];

            for message in messages {
                let processor_msg = ProcessorMessage::CosmwasmExecuteMsg { msg: message };

                encoded_messages.push(processor_msg);
            }

            let tx_resp = self
                .neutron_client
                .execute_wasm(
                    &self.cfg.neutron.authorizations,
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

            self.neutron_client.poll_for_tx(&tx_resp.hash).await?;

            Ok(())
        }

        /// ticks the processor on neutron
        pub async fn tick_neutron(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
            let tx_resp = self
                .neutron_client
                .execute_wasm(
                    &self.cfg.neutron.processor,
                    valence_processor_utils::msg::ExecuteMsg::PermissionlessAction(
                        valence_processor_utils::msg::PermissionlessMsg::Tick {},
                    ),
                    vec![],
                    None,
                )
                .await?;

            debug!("tx hash: {}", tx_resp.hash);

            self.neutron_client.poll_for_tx(&tx_resp.hash).await?;

            Ok(())
        }
    }
}
