use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct ChainClientInputs {
    pub grpc_url: String,
    pub grpc_port: String,
    pub chain_id: String,
    pub chain_denom: String,
}
