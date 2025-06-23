use std::{env, error::Error, fs};

use alloy::{
    hex::FromHex,
    primitives::{Address, FixedBytes},
};
use packages::types::sol_types::Authorization;
use serde::Deserialize;
use sp1_sdk::{HashableKey, SP1VerifyingKey};
use valence_domain_clients::{
    clients::{coprocessor::CoprocessorClient, ethereum::EthereumClient},
    coprocessor::base_client::CoprocessorBaseClient,
    evm::{base_client::EvmBaseClient, request_provider_client::RequestProviderClient},
};
use wbtc_deploy::DIR;
use wbtc_types::ethereum_config::EthereumStrategyConfig;

#[derive(Deserialize, Debug)]
struct Parameters {
    general: General,
    vault: Vault,
    coprocessor_app: CoprocessorApp,
}

#[derive(Deserialize, Debug)]
struct General {
    rpc_url: String,
}

#[derive(Deserialize, Debug)]
struct Vault {
    strategist: Address,
}

#[derive(Deserialize, Debug)]
struct CoprocessorApp {
    eureka_transfer_coprocessor_app_id: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv::dotenv().ok();
    let mnemonic = env::var("MNEMONIC").expect("mnemonic must be provided");

    // Read ethereum.toml from the deploy directory
    let current_dir = env::current_dir()?;
    let parameters = fs::read_to_string(current_dir.join(format!("{DIR}/ethereum.toml")))
        .expect("Failed to read file");
    let parameters: Parameters = toml::from_str(&parameters).expect("Failed to parse TOML");

    let eth_stg_cfg =
        fs::read_to_string(current_dir.join(format!("{DIR}/ethereum_strategy_config.toml")))
            .expect("Failed to read file");

    let eth_stg_cfg: EthereumStrategyConfig =
        toml::from_str(&eth_stg_cfg).expect("Failed to parse TOML");

    let eth_client = EthereumClient::new(&parameters.general.rpc_url, &mnemonic, None)?;
    let rp = eth_client.get_request_provider().await?;

    let authorization = Authorization::new(eth_stg_cfg.authorizations, &rp);

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
    let program_vk = FixedBytes::<32>::from_hex(sp1_program_vk.bytes32()).unwrap();
    let registries = vec![0]; // Only one and IBC Eureka app will use registry 0
    let authorized_addresses = vec![parameters.vault.strategist];
    let vks = vec![program_vk];

    // Remember we send arrays because we allow  multiple registries added at once
    let tx = authorization
        .addRegistries(registries, vec![authorized_addresses], vks, vec![false])
        .into_transaction_request();

    // Send the transaction
    eth_client.sign_and_send(tx).await?;
    println!("Authorization created successfully");

    // TODO: Keep the ownership fo the authorization contract for now but we should transfer it eventually

    Ok(())
}
