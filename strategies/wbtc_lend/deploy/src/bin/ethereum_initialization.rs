use std::{env, error::Error, fs};

use alloy::{primitives::FixedBytes, sol_types::SolCall};
use alloy::hex::FromHex;
use alloy::primitives::Bytes;
use wbtc_lend_deploy::{INPUTS_DIR, OUTPUTS_DIR};
use wbtc_lend_types::ethereum_config::EthereumStrategyConfig;
use packages::{
    labels::CCTP_TRANSFER_LABEL,
    types::{
        inputs::VaultInput,
        sol_types::{
            Authorization::{self, AuthorizationData},
            CCTPTransfer,
        },
    },
};
use serde::Deserialize;
use sp1_sdk::{HashableKey, SP1VerifyingKey};
use valence_domain_clients::{
    clients::ethereum::EthereumClient,
    evm::{base_client::EvmBaseClient, request_provider_client::RequestProviderClient},
};
use valence_domain_clients::clients::coprocessor::CoprocessorClient;
use valence_domain_clients::coprocessor::base_client::CoprocessorBaseClient;
use packages::types::inputs::EurekaTransferCoprocessorApp;
use packages::types::sol_types::Authorization::ZkAuthorizationData;
use packages::verification::{VALENCE_ETHEREUM_VERIFICATION_ROUTER, VERIFICATION_ROUTE};

#[derive(Deserialize, Debug)]
struct Parameters {
    general: General,
    vault: VaultInput,
    coprocessor_app: EurekaTransferCoprocessorApp,
}

#[derive(Deserialize, Debug)]
struct General {
    rpc_url: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let mnemonic = env::var("MNEMONIC").expect("mnemonic must be provided");

    // Read ethereum.toml from the deploy directory
    let current_dir = env::current_dir()?;
    let parameters = fs::read_to_string(current_dir.join(format!("{INPUTS_DIR}/ethereum.toml")))
        .expect("Failed to read file");
    let parameters: Parameters = toml::from_str(&parameters).expect("Failed to parse TOML");

    let eth_stg_cfg = fs::read_to_string(
        current_dir.join(format!("{OUTPUTS_DIR}/ethereum_strategy_config.toml")),
    )
        .expect("Failed to read file");

    let eth_stg_cfg: EthereumStrategyConfig =
        toml::from_str(&eth_stg_cfg).expect("Failed to parse TOML");

    let eth_client = EthereumClient::new(&parameters.general.rpc_url, &mnemonic, None)?;
    let rp = eth_client.get_request_provider().await?;

    let authorization = Authorization::new(eth_stg_cfg.authorizations, &rp);

    let set_verification_router_tx = authorization
        .setVerificationRouter(VALENCE_ETHEREUM_VERIFICATION_ROUTER.parse()?)
        .into_transaction_request();

    // Send the transaction
    eth_client.sign_and_send(set_verification_router_tx).await?;
    println!("Verification router set successfully");

    // Get the VK for the coprocessor app
    let coprocessor_client = CoprocessorClient::default();
    let program_vk = coprocessor_client
        .get_vk(
            &parameters
                .coprocessor_app
                .eureka_transfer_coprocessor_app_id,
        )
        .await?;

    let sp1_program_vk: SP1VerifyingKey = bincode::deserialize(&program_vk)?;
    let program_vk = Bytes::from_hex(sp1_program_vk.bytes32()).unwrap();
    let registries = vec![0]; // Only one and IBC Eureka app will use registry 0
    let authorized_addresses = vec![parameters.vault.strategist];

    let zk_authorization_data = ZkAuthorizationData {
        allowedExecutionAddresses: authorized_addresses.clone(),
        vk: program_vk,
        route: VERIFICATION_ROUTE.to_string(),
        validateBlockNumberExecution: false,
        metadataHash: FixedBytes::default(),
    };

    // Remember we send arrays because we allow  multiple registries added at once
    let tx = authorization
        .addRegistries(registries, vec![zk_authorization_data])
        .into_transaction_request();

    // Send the transaction
    eth_client.sign_and_send(tx).await?;
    println!("Authorization created successfully");

    // TODO: Keep the ownership of the authorization contract for now but we should transfer it eventually

    Ok(())
}
