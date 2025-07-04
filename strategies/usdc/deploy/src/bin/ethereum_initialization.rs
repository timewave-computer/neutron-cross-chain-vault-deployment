use std::{env, error::Error, fs};

use alloy::{
    primitives::{Address, FixedBytes},
    sol_types::SolCall,
};
use packages::{
    labels::CCTP_TRANSFER_LABEL,
    types::sol_types::{
        Authorization::{self, AuthorizationData},
        CCTPTransfer,
    },
};
use serde::Deserialize;
use usdc_deploy::{INPUTS_DIR, OUTPUTS_DIR};
use usdc_types::ethereum_config::EthereumStrategyConfig;
use valence_domain_clients::{
    clients::ethereum::EthereumClient,
    evm::{base_client::EvmBaseClient, request_provider_client::RequestProviderClient},
};

#[derive(Deserialize, Debug)]
struct Parameters {
    general: General,
    vault: Vault,
}

#[derive(Deserialize, Debug)]
struct General {
    rpc_url: String,
}

#[derive(Deserialize, Debug)]
struct Vault {
    strategist: Address,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
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

    let labels = vec![CCTP_TRANSFER_LABEL.to_string()];
    let users = vec![parameters.vault.strategist];
    let authorization_data = vec![AuthorizationData {
        contractAddress: eth_stg_cfg.libraries.cctp_transfer,
        useFunctionSelector: true,
        functionSelector: FixedBytes::<4>::new(CCTPTransfer::transferCall::SELECTOR),
        callHash: FixedBytes::<32>::default(),
    }];

    let tx = authorization
        .addStandardAuthorizations(labels, vec![users], vec![authorization_data])
        .into_transaction_request();

    // Send the transaction
    eth_client.sign_and_send(tx).await?;
    println!("Authorization created successfully");

    // TODO: Keep the ownership of the authorization contract for now but we should transfer it eventually

    Ok(())
}
