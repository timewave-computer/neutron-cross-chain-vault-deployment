use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct ChainClientInputs {
    pub grpc_url: String,
    pub grpc_port: String,
    pub chain_id: String,
    pub chain_denom: String,
}

#[derive(Deserialize, Debug)]
pub struct ClearingQueueCoprocessorApp {
    pub clearing_queue_coprocessor_app_id: String,
}

#[derive(Deserialize, Debug)]
pub struct EurekaTransferCoprocessorApp {
    pub eureka_transfer_coprocessor_app_id: String,
}
