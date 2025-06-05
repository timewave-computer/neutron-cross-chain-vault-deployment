use std::{env, error::Error, fs};

use alloy::primitives::{Address, FixedBytes};
use serde::Deserialize;
use types::{ethereum_config::EthereumStrategyConfig, sol_types::Authorization};
use valence_domain_clients::{
    clients::ethereum::EthereumClient,
    evm::{base_client::EvmBaseClient, request_provider_client::RequestProviderClient},
};

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
    eureka_transfer_coprocessor_app_vk: FixedBytes<32>,
    domain_vk: FixedBytes<32>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv::dotenv().ok();
    let mnemonic = env::var("MNEMONIC").expect("mnemonic must be provided");

    // Read ethereum.toml from the deploy directory
    let current_dir = env::current_dir()?;
    let parameters = fs::read_to_string(current_dir.join("deploy/src/ethereum.toml"))
        .expect("Failed to read file");
    let parameters: Parameters = toml::from_str(&parameters).expect("Failed to parse TOML");

    let eth_stg_cfg =
        fs::read_to_string(current_dir.join("deploy/src/ethereum_strategy_config.toml"))
            .expect("Failed to read file");

    let eth_stg_cfg: EthereumStrategyConfig =
        toml::from_str(&eth_stg_cfg).expect("Failed to parse TOML");

    let eth_client = EthereumClient::new(&parameters.general.rpc_url, &mnemonic, None)?;
    let rp = eth_client.get_request_provider().await?;

    let authorization = Authorization::new(eth_stg_cfg.authorizations, &rp);

    let registries = vec![0]; // Only one and IBC Eureka app will use registry 0
    let authorized_addresses = vec![parameters.vault.strategist];
    let vks = vec![
        parameters
            .coprocessor_app
            .eureka_transfer_coprocessor_app_vk,
    ];
    let domain_vk = parameters.coprocessor_app.domain_vk;

    // Remember we send arrays because we allow  multiple registries added at once
    let tx = authorization
        .addRegistries(
            registries,
            vec![authorized_addresses],
            vks,
            domain_vk,
            vec![false],
        )
        .into_transaction_request();

    // Send the transaction
    eth_client.sign_and_send(tx).await?;
    println!("Authorization created successfully");

    // TODO: Keep the ownership fo the authorization contract for now but we should transfer it eventually

    Ok(())
}
