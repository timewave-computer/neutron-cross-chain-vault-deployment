pub(crate) mod neutron {
    use std::error::Error;

    use cosmwasm_std::to_json_binary;
    use serde::Serialize;
    use valence_authorization_utils::msg::ProcessorMessage;
    use valence_domain_clients::cosmos::wasm_client::WasmClient;

    use crate::strategy_config::Strategy;

    impl Strategy {
        /// enqueues a message on neutron
        pub async fn enqueue_neutron<T>(
            &mut self,
            label: &str,
            messages: Vec<T>,
        ) -> Result<(), Box<dyn Error + Send + Sync>>
        where
            T: Serialize,
        {
            let mut encoded_messages = vec![];

            for message in messages {
                let encoded_msg = to_json_binary(&message)?;

                let processor_msg = ProcessorMessage::CosmwasmExecuteMsg { msg: encoded_msg };

                encoded_messages.push(processor_msg);
            }

            self.neutron_client
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

            Ok(())
        }

        /// ticks the processor on neutron
        pub async fn tick_neutron(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
            self.neutron_client
                .execute_wasm(
                    &self.cfg.neutron.processor,
                    valence_processor_utils::msg::ExecuteMsg::PermissionlessAction(
                        valence_processor_utils::msg::PermissionlessMsg::Tick {},
                    ),
                    vec![],
                    None,
                )
                .await?;

            Ok(())
        }
    }
}
