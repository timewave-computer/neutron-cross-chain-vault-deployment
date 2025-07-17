use std::{env, error::Error, fs, time::SystemTime};

use cosmwasm_std::{Decimal, Uint128};
use packages::{
    contracts::{PATH_NEUTRON_CODE_IDS, UploadedContracts},
    types::inputs::ChainClientInputs,
    verification::VALENCE_NEUTRON_VERIFICATION_GATEWAY,
};
use serde::Deserialize;
use usdc_deploy::{INPUTS_DIR, OUTPUTS_DIR};
use usdc_types::{
    neutron_config::{
        NeutronAccounts, NeutronCoprocessorAppIds, NeutronDenoms, NeutronLibraries,
        NeutronStrategyConfig,
    },
    noble_config::NobleStrategyConfig,
};
use valence_clearing_queue_supervaults::msg::SupervaultSettlementInfo;
use valence_domain_clients::{
    clients::neutron::NeutronClient,
    cosmos::{grpc_client::GrpcSigningClient, wasm_client::WasmClient},
};

use valence_library_utils::LibraryAccountType;

#[derive(Deserialize, Debug)]
struct Parameters {
    general: General,
    program: Program,
    coprocessor_app: CoprocessorApp,
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
    supervault: String,
    supervault_asset1: String,
    supervault_asset2: String,
    supervault_lp_denom: String,
}

#[derive(Deserialize, Debug)]
struct CoprocessorApp {
    clearing_queue_coprocessor_app_id: String,
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
    let code_id_supervaults_lper = *uploaded_contracts.code_ids.get("supervaults_lper").unwrap();
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
            valence_authorization_utils::msg::PermissionedMsg::SetVerificationGateway {
                verification_gateway: VALENCE_NEUTRON_VERIFICATION_GATEWAY.to_string(),
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
    let supervaults_lper_config = valence_supervaults_lper::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[0].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[1].clone()),
        vault_addr: params.program.supervault.clone(),
        lp_config: valence_supervaults_lper::msg::LiquidityProviderConfig {
            asset_data: valence_library_utils::liquidity_utils::AssetData {
                asset1: params.program.supervault_asset1.clone(),
                asset2: params.program.supervault_asset2.clone(),
            },
            lp_denom: params.program.supervault_lp_denom.clone(),
        },
    };

    let instantiate_supervaults_lper_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_supervaults_lper::msg::LibraryConfig,
    > {
        owner: processor_address.clone(),
        processor: processor_address.clone(),
        config: supervaults_lper_config,
    };
    let supervaults_lper_library_address = neutron_client
        .instantiate(
            code_id_supervaults_lper,
            "supervaults_lper".to_string(),
            instantiate_supervaults_lper_msg,
            None,
        )
        .await?;
    println!("Supervaults lper library instantiated: {supervaults_lper_library_address}");

    // Finally instantiate the clearing queue library
    let clearing_config = valence_clearing_queue_supervaults::msg::LibraryConfig {
        settlement_acc_addr: LibraryAccountType::Addr(predicted_base_accounts[1].clone()),
        denom: params.program.deposit_token_on_neutron_denom.clone(),
        latest_id: None,
        mars_settlement_ratio: Decimal::zero(), // 0% because there is no mars lending being done
        supervaults_settlement_info: vec![SupervaultSettlementInfo {
            supervault_addr: params.program.supervault.clone(),
            supervault_sender: predicted_base_accounts[0].clone(), // Input account of supervaults lper library
            settlement_ratio: Decimal::one(), // 100% because there is only one supervault and everything goes to it
        }],
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
        approved_libraries: vec![supervaults_lper_library_address.clone()],
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
        supervault_lp: params.program.supervault_lp_denom.clone(),
    };

    let accounts = NeutronAccounts {
        deposit: deposit_account_address,
        settlement: settlement_account_address,
    };

    let libraries = NeutronLibraries {
        supervault_lper: supervaults_lper_library_address,
        clearing_queue: clearing_queue_library_address,
    };

    let coprocessor_app_ids = NeutronCoprocessorAppIds {
        clearing_queue: params.coprocessor_app.clearing_queue_coprocessor_app_id,
    };

    let neutron_cfg = NeutronStrategyConfig {
        grpc_url: params.general.grpc_url.clone(),
        grpc_port: params.general.grpc_port.clone(),
        chain_id: params.general.chain_id.clone(),
        supervault: params.program.supervault.clone(),
        denoms,
        accounts,
        libraries,
        min_ibc_fee: Uint128::one(),
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
        grpc_url: noble_inputs.grpc_url.to_string(),
        grpc_port: noble_inputs.grpc_port.to_string(),
        chain_id: noble_inputs.chain_id.to_string(),
        chain_denom: noble_inputs.chain_denom.to_string(),
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
