use std::{collections::HashMap, env, error::Error, fs};

use serde::{Deserialize, Serialize};
use usdc_deploy::DIR;
use valence_domain_clients::{clients::neutron::NeutronClient, cosmos::wasm_client::WasmClient};

const GRPC_URL: &str = "https://rpc.neutron.quokkastake.io";
const GRPC_PORT: &str = "9090";
const CHAIN_ID: &str = "neutron-1";

#[derive(Deserialize, Serialize)]
struct UploadedContracts {
    code_ids: HashMap<String, u64>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv::dotenv().ok();
    let mnemonic = env::var("MNEMONIC").expect("mnemonic must be provided");

    let neutron_client = NeutronClient::new(GRPC_URL, GRPC_PORT, &mnemonic, CHAIN_ID).await?;

    let mut uploaded_contracts = UploadedContracts {
        code_ids: HashMap::new(),
    };

    // Upload all contracts that are in /packages/src/contracts/cw
    for entry in std::fs::read_dir("./packages/src/contracts/cw")? {
        let entry = entry?;
        if entry.path().is_file() {
            let contract_name = entry.file_name().to_string_lossy().to_string();
            println!("Uploading contract: {}", contract_name);
            let code_id = neutron_client
                .upload_code(entry.path().to_str().unwrap())
                .await?;

            // Remove .wasm extension and valence_ prefix
            let clean_name = contract_name
                .strip_prefix("valence_")
                .unwrap_or(&contract_name);
            let clean_name = clean_name
                .strip_suffix(".wasm")
                .unwrap_or(clean_name)
                .to_string();

            uploaded_contracts.code_ids.insert(clean_name, code_id);
        }
    }

    let toml_content = toml::to_string_pretty(&uploaded_contracts)?;
    let current_dir = env::current_dir()?;
    fs::write(
        current_dir.join(format!("{DIR}/neutron_code_ids.toml")),
        toml_content.clone(),
    )?;

    println!("Contracts uploaded!");
    Ok(())
}
