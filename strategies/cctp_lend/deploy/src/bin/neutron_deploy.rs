use std::{env, error::Error, fs, time::SystemTime};

use cctp_lend_deploy::{INPUTS_DIR, OUTPUTS_DIR};
use cctp_lend_types::{
    neutron_config::{
        NeutronAccounts, NeutronCoprocessorAppIds, NeutronDenoms, NeutronLibraries,
        NeutronStrategyConfig,
    },
    noble_config::NobleStrategyConfig,
};
use cosmwasm_std::Decimal;
use packages::{
    contracts::{PATH_NEUTRON_CODE_IDS, UploadedContracts},
    types::inputs::{ChainClientInputs, ClearingQueueCoprocessorApp},
    verification::VALENCE_NEUTRON_VERIFICATION_ROUTER,
};
use serde::Deserialize;
use valence_domain_clients::{
    clients::neutron::NeutronClient,
    cosmos::{grpc_client::GrpcSigningClient, wasm_client::WasmClient},
};

use valence_library_utils::LibraryAccountType;

#[derive(Deserialize, Debug)]
struct Parameters {
    general: General,
    program: Program,
    coprocessor_app: ClearingQueueCoprocessorApp,
}

#[derive(Deserialize, Debug)]
struct General {
    grpc_url: String,
    grpc_port: String,
    chain_id: String,
    owner: String,
}

#[derive(Deserialize, Debug)]
struct Program {
    deposit_token_on_neutron_denom: String,
    mars_credit_manager: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv::dotenv().ok();
    let mnemonic = env::var("MNEMONIC").expect("mnemonic must be provided");

    let current_dir = env::current_dir()?;

    let parameters = fs::read_to_string(current_dir.join(format!("{INPUTS_DIR}/neutron.toml")))
        .expect("Failed to read file");

    let params: Parameters = toml::from_str(&parameters).expect("Failed to parse TOML");

    // Read code IDS from the code ids file
    let code_ids_content = fs::read_to_string(current_dir.join(PATH_NEUTRON_CODE_IDS))
        .expect("Failed to read code ids file");
    let uploaded_contracts: UploadedContracts =
        toml::from_str(&code_ids_content).expect("Failed to parse code ids");

    let neutron_client = NeutronClient::new(
        &params.general.grpc_url,
        &params.general.grpc_port,
        &mnemonic,
        &params.general.chain_id,
    )
    .await?;

    let my_address = neutron_client
        .get_signing_client()
        .await?
        .address
        .to_string();

    // Get all code IDs
    let code_id_authorization = *uploaded_contracts.code_ids.get("authorization").unwrap();
    let code_id_processor = *uploaded_contracts.code_ids.get("processor").unwrap();
    let code_id_mars_lending = *uploaded_contracts.code_ids.get("mars_lending").unwrap();
    let code_id_clearing_queue = *uploaded_contracts
        .code_ids
        .get("clearing_queue_supervaults")
        .unwrap();
    let code_id_base_account = *uploaded_contracts.code_ids.get("base_account").unwrap();

    let now = SystemTime::now();
    let salt_raw = now
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs()
        .to_string();
    let salt = hex::encode(salt_raw.as_bytes());

    let predicted_processor_address = neutron_client
        .predict_instantiate2_addr(code_id_processor, salt.clone(), my_address.clone())
        .await?
        .address;

    // Owner will initially be the deploy address and eventually will be transferred to the owned address
    let authorization_instantiate_msg = valence_authorization_utils::msg::InstantiateMsg {
        owner: my_address.clone(),
        sub_owners: vec![],
        processor: predicted_processor_address.clone(),
    };

    let authorization_address = neutron_client
        .instantiate2(
            code_id_authorization,
            "authorization".to_string(),
            authorization_instantiate_msg,
            Some(params.general.owner.clone()),
            salt.clone(),
        )
        .await?;
    println!("Authorization instantiated: {authorization_address}");

    let processor_instantiate_msg = valence_processor_utils::msg::InstantiateMsg {
        authorization_contract: authorization_address.clone(),
        polytone_contracts: None,
    };

    let processor_address = neutron_client
        .instantiate2(
            code_id_processor,
            "processor".to_string(),
            processor_instantiate_msg,
            Some(params.general.owner.clone()),
            salt.clone(),
        )
        .await?;
    println!("Processor instantiated: {processor_address}");

    // Set the verification gateway address on the authorization contract
    let set_verification_gateway_msg =
        valence_authorization_utils::msg::ExecuteMsg::PermissionedAction(
            valence_authorization_utils::msg::PermissionedMsg::SetVerificationRouter {
                address: VALENCE_NEUTRON_VERIFICATION_ROUTER.to_string(),
            },
        );

    neutron_client
        .execute_wasm(
            &authorization_address,
            set_verification_gateway_msg,
            vec![],
            None,
        )
        .await?;

    // Predict all base accounts, we are going to store all salts for them as well. In total we need only 2
    let mut salts = vec![];
    let mut predicted_base_accounts = vec![];
    for i in 0..2 {
        let salt = hex::encode(format!("{salt_raw}{i}").as_bytes());
        salts.push(salt.clone());
        let predicted_base_account_address = neutron_client
            .predict_instantiate2_addr(code_id_base_account, salt.clone(), my_address.clone())
            .await?
            .address;
        println!("Predicted base account address {i}: {predicted_base_account_address}");
        predicted_base_accounts.push(predicted_base_account_address);
    }

    // Instantiate supervaults lper library
    let mars_lend_config = valence_mars_lending::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[0].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[1].clone()),
        credit_manager_addr: params.program.mars_credit_manager.to_string(),
        denom: params.program.deposit_token_on_neutron_denom.to_string(),
    };

    let instantiate_mars_lending_msg =
        valence_library_utils::msg::InstantiateMsg::<valence_mars_lending::msg::LibraryConfig> {
            owner: processor_address.clone(),
            processor: processor_address.clone(),
            config: mars_lend_config,
        };
    let mars_lending_library_address = neutron_client
        .instantiate(
            code_id_mars_lending,
            "mars_lending".to_string(),
            instantiate_mars_lending_msg,
            None,
        )
        .await?;
    println!("Mars lending library instantiated: {mars_lending_library_address}");

    // Finally instantiate the clearing queue library
    let clearing_config = valence_clearing_queue_supervaults::msg::LibraryConfig {
        settlement_acc_addr: LibraryAccountType::Addr(predicted_base_accounts[1].clone()),
        denom: params.program.deposit_token_on_neutron_denom.clone(),
        latest_id: None,
        mars_settlement_ratio: Decimal::one(), // 100%, all goes to mars
        supervaults_settlement_info: vec![],   // no supervaults positions
    };
    let instantiate_clearing_queue_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_clearing_queue_supervaults::msg::LibraryConfig,
    > {
        owner: processor_address.clone(),
        processor: processor_address.clone(),
        config: clearing_config,
    };

    let clearing_queue_library_address = neutron_client
        .instantiate(
            code_id_clearing_queue,
            "clearing_queue".to_string(),
            instantiate_clearing_queue_msg,
            Some(params.general.owner.clone()),
        )
        .await?;
    println!("Clearing queue library instantiated: {clearing_queue_library_address}");

    // Now the rest
    let deposit_account = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![mars_lending_library_address.clone()],
    };
    let deposit_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "deposit".to_string(),
            deposit_account,
            Some(params.general.owner.clone()),
            salts[0].clone(),
        )
        .await?;
    println!("Deposit account instantiated: {deposit_account_address}");

    let settlement_account = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![
            // This will contain all libraries that will execute actions on the settlement account
            clearing_queue_library_address.clone(),
        ],
    };
    let settlement_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "settlement".to_string(),
            settlement_account,
            Some(params.general.owner.clone()),
            salts[1].clone(),
        )
        .await?;
    println!("Settlement account instantiated: {settlement_account_address}");

    let denoms = NeutronDenoms {
        deposit_token: params.program.deposit_token_on_neutron_denom,
        ntrn: "untrn".to_string(),
    };

    let accounts = NeutronAccounts {
        deposit: deposit_account_address,
        settlement: settlement_account_address,
    };

    let libraries = NeutronLibraries {
        mars_lending: mars_lending_library_address,
        clearing_queue: clearing_queue_library_address,
    };

    let coprocessor_app_ids = NeutronCoprocessorAppIds {
        clearing_queue: params.coprocessor_app.clearing_queue_coprocessor_app_id,
    };

    let neutron_cfg = NeutronStrategyConfig {
        grpc_url: params.general.grpc_url.clone(),
        grpc_port: params.general.grpc_port.clone(),
        chain_id: params.general.chain_id.clone(),
        mars_credit_manager: params.program.mars_credit_manager.clone(),
        denoms,
        accounts,
        libraries,
        authorizations: authorization_address,
        processor: processor_address,
        coprocessor_app_ids,
    };

    println!("Neutron Strategy Config created successfully");

    // Save the Neutron Strategy Config to a toml file
    let neutron_cfg_toml =
        toml::to_string(&neutron_cfg).expect("Failed to serialize Neutron Strategy Config");
    fs::write(
        current_dir.join(format!("{OUTPUTS_DIR}/neutron_strategy_config.toml")),
        neutron_cfg_toml,
    )
    .expect("Failed to write Neutron Strategy Config to file");

    let noble_inputs = fs::read_to_string(current_dir.join(format!("{INPUTS_DIR}/noble.toml")))
        .expect("Failed to read file");

    let noble_inputs: ChainClientInputs =
        toml::from_str(&noble_inputs).expect("Failed to parse noble toml inputs");

    let noble_cfg = NobleStrategyConfig {
        grpc_url: noble_inputs.grpc_url,
        grpc_port: noble_inputs.grpc_port,
        chain_id: noble_inputs.chain_id,
        chain_denom: noble_inputs.chain_denom,
        forwarding_account: "noble_forwarding_account".to_string(),
    };

    // Write the Noble strategy config to a file
    let noble_cfg_path = current_dir.join(format!("{OUTPUTS_DIR}/noble_strategy_config.toml"));
    fs::write(
        noble_cfg_path,
        toml::to_string(&noble_cfg).expect("Failed to serialize Noble strategy config"),
    )
    .expect("Failed to write Noble strategy config to file");

    Ok(())
}
