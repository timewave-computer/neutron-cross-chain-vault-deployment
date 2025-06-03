use std::{
    collections::{BTreeMap, HashMap},
    env,
    error::Error,
    fs,
    time::SystemTime,
};

use cosmwasm_std::{Uint128, Uint64};
use serde::Deserialize;
use types::neutron_config::{
    NeutronAccounts, NeutronDenoms, NeutronLibraries, NeutronStrategyConfig,
};
use valence_domain_clients::{
    clients::neutron::NeutronClient,
    cosmos::{grpc_client::GrpcSigningClient, wasm_client::WasmClient},
};
use valence_forwarder_library::msg::{ForwardingConstraints, UncheckedForwardingConfig};
use valence_library_utils::{denoms::UncheckedDenom, LibraryAccountType};

#[derive(Deserialize, Debug)]
struct UploadedContracts {
    code_ids: HashMap<String, u64>,
}

#[derive(Deserialize, Debug)]
struct Parameters {
    general: General,
    ica: Ica,
    program: Program,
}

#[derive(Deserialize, Debug)]
struct General {
    grpc_url: String,
    grpc_port: String,
    chain_id: String,
    owner: String,
}

#[derive(Deserialize, Debug)]
struct Ica {
    deposit_token_on_hub_denom: String,
    channel_id: String,
    ibc_transfer_timeout: u64,
    connection_id: String,
    ica_timeout: u64,
}

#[derive(Deserialize, Debug)]
struct Program {
    deposit_token_on_neutron_denom: String,
    mars_credit_manager: String,
    supervault: String,
    supervault_asset1: String,
    supervault_asset2: String,
    supervault_lp_denom: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv::dotenv().ok();
    let mnemonic = env::var("MNEMONIC").expect("mnemonic must be provided");

    let current_dir = env::current_dir()?;
    let parameters = fs::read_to_string(current_dir.join("deploy/src/neutron.toml"))
        .expect("Failed to read file");

    let params: Parameters = toml::from_str(&parameters).expect("Failed to parse TOML");

    // Read code IDS from the code ids file
    let code_ids_content = fs::read_to_string(current_dir.join("deploy/src/neutron_code_ids.toml"))
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
    let code_id_ica_ibc_transfer_library =
        *uploaded_contracts.code_ids.get("ica_ibc_transfer").unwrap();
    let code_id_forwarder_library = *uploaded_contracts
        .code_ids
        .get("forwarder_library")
        .unwrap();
    let code_id_mars_lending = *uploaded_contracts.code_ids.get("mars_lending").unwrap();
    let code_id_supervaults_lper = *uploaded_contracts.code_ids.get("supervaults_lper").unwrap();
    let code_id_clearing_queue = *uploaded_contracts.code_ids.get("clearing_queue").unwrap();
    let code_id_base_account = *uploaded_contracts.code_ids.get("base_account").unwrap();
    let code_id_interchain_account = *uploaded_contracts
        .code_ids
        .get("interchain_account")
        .unwrap();
    let code_id_verification_gateway = *uploaded_contracts
        .code_ids
        .get("verification_gateway")
        .unwrap();

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
    println!("Authorization instantiated: {}", authorization_address);

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
    println!("Processor instantiated: {}", processor_address);

    // Instantiate the verification gateway
    let instantiate_verification_gateway_msg = valence_verification_gateway::msg::InstantiateMsg {};
    let verification_gateway = neutron_client
        .instantiate(
            code_id_verification_gateway,
            "verification-gateway".to_string(),
            instantiate_verification_gateway_msg,
            None,
        )
        .await?;

    // Set the verification gateway address on the authorization contract
    let set_verification_gateway_msg =
        valence_authorization_utils::msg::ExecuteMsg::PermissionedAction(
            valence_authorization_utils::msg::PermissionedMsg::SetVerificationGateway {
                verification_gateway,
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

    // Predict all base accounts, we are going to store all salts for them as well. In total we need 4
    let mut salts = vec![];
    let mut predicted_base_accounts = vec![];
    for i in 0..4 {
        let salt = hex::encode(format!("{}{}", salt_raw, i).as_bytes());
        salts.push(salt.clone());
        let predicted_base_account_address = neutron_client
            .predict_instantiate2_addr(code_id_base_account, salt.clone(), my_address.clone())
            .await?
            .address;
        println!(
            "Predicted base account address {}: {}",
            i, predicted_base_account_address
        );
        predicted_base_accounts.push(predicted_base_account_address);
    }

    // Predict the valence ICA address
    let predicted_valence_ica_address = neutron_client
        .predict_instantiate2_addr(code_id_interchain_account, salt.clone(), my_address.clone())
        .await?
        .address;

    // Instantiate the ICA ibc transfer library
    let config = valence_ica_ibc_transfer::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_valence_ica_address.clone()),
        // The strategist needs to update this to the actual amount that needs to be transferred. There will be an authorization for this.
        amount: Uint128::one(),
        denom: params.ica.deposit_token_on_hub_denom.clone(),
        receiver: predicted_base_accounts[0].clone(),
        memo: "".to_string(),
        remote_chain_info: valence_ica_ibc_transfer::msg::RemoteChainInfo {
            channel_id: params.ica.channel_id,
            ibc_transfer_timeout: Some(params.ica.ibc_transfer_timeout),
        },
        denom_to_pfm_map: BTreeMap::new(),
        eureka_config: None,
    };

    let instantiate_ica_ibc_transfer_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_ica_ibc_transfer::msg::LibraryConfig,
    > {
        owner: params.general.owner.clone(),
        processor: processor_address.clone(),
        config,
    };

    // Instantiate the ICA IBC transfer library
    let ica_ibc_transfer_library_address = neutron_client
        .instantiate(
            code_id_ica_ibc_transfer_library,
            "ica_ibc_transfer".to_string(),
            instantiate_ica_ibc_transfer_msg,
            None,
        )
        .await?;
    println!(
        "ICA IBC Transfer library instantiated: {}",
        ica_ibc_transfer_library_address
    );

    // Instantiate the deposit forwarder library
    // This library will have Mars deposit account as output in phase 1 and supervault deposit account in phase 2
    let deposit_forwarder_config = valence_forwarder_library::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[0].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[1].clone()),
        forwarding_configs: vec![UncheckedForwardingConfig {
            denom: UncheckedDenom::Native(params.program.deposit_token_on_neutron_denom.clone()),
            max_amount: Uint128::MAX,
        }],
        forwarding_constraints: ForwardingConstraints::default(),
    };

    let instantiate_deposit_forwarder_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_forwarder_library::msg::LibraryConfig,
    > {
        owner: params.general.owner.clone(),
        processor: processor_address.clone(),
        config: deposit_forwarder_config,
    };

    let deposit_forwarder_library_address = neutron_client
        .instantiate(
            code_id_forwarder_library,
            "deposit_forwarder".to_string(),
            instantiate_deposit_forwarder_msg,
            None,
        )
        .await?;
    println!(
        "Deposit forwarder library instantiated: {}",
        deposit_forwarder_library_address
    );

    // Instantiate the Mars lending library
    // In Phase 1 the output account is the settlement account and in phase 2 this will be initial deposit account
    let mars_lending_config = valence_mars_lending::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[1].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[3].clone()),
        credit_manager_addr: params.program.mars_credit_manager,
        denom: params.program.deposit_token_on_neutron_denom.clone(),
    };

    let instantiate_mars_lending_msg =
        valence_library_utils::msg::InstantiateMsg::<valence_mars_lending::msg::LibraryConfig> {
            owner: params.general.owner.clone(),
            processor: processor_address.clone(),
            config: mars_lending_config,
        };

    let mars_lending_library_address = neutron_client
        .instantiate(
            code_id_mars_lending,
            "mars_lending".to_string(),
            instantiate_mars_lending_msg,
            None,
        )
        .await?;
    println!(
        "Mars lending library instantiated: {}",
        mars_lending_library_address
    );

    // Instantiate supervaults lper library
    let supervaults_lper_config = valence_supervaults_lper::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[2].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[3].clone()),
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
        owner: params.general.owner.clone(),
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
    println!(
        "Supervaults lper library instantiated: {}",
        supervaults_lper_library_address
    );

    // Finally instantiate the clearing queue library
    let clearing_config = valence_clearing_queue::msg::LibraryConfig {
        settlement_acc_addr: LibraryAccountType::Addr(predicted_base_accounts[3].clone()),
        denom: params.program.deposit_token_on_neutron_denom.clone(),
        latest_id: None,
    };
    let instantiate_clearing_queue_msg =
        valence_library_utils::msg::InstantiateMsg::<valence_clearing_queue::msg::LibraryConfig> {
            owner: params.general.owner.clone(),
            processor: processor_address.clone(),
            config: clearing_config,
        };

    let clearing_queue_library_address = neutron_client
        .instantiate(
            code_id_clearing_queue,
            "clearing_queue".to_string(),
            instantiate_clearing_queue_msg,
            None,
        )
        .await?;
    println!(
        "Clearing queue library instantiated: {}",
        clearing_queue_library_address
    );

    // Instantiate all acounts now
    // First the ICA
    let valence_ica_instantiate_msg = valence_account_utils::ica::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![ica_ibc_transfer_library_address.clone()],
        remote_domain_information: valence_account_utils::ica::RemoteDomainInfo {
            connection_id: params.ica.connection_id.clone(),
            ica_timeout_seconds: Uint64::from(params.ica.ica_timeout),
        },
    };
    let valence_ica_address = neutron_client
        .instantiate2(
            code_id_interchain_account,
            "valence_ica".to_string(),
            valence_ica_instantiate_msg,
            Some(params.general.owner.clone()),
            salt.clone(),
        )
        .await?;
    println!("Valence ICA instantiated: {}", valence_ica_address);

    // Now the rest
    let ica_deposit_account = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![deposit_forwarder_library_address.clone()],
    };
    let ica_deposit_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "ica_deposit".to_string(),
            ica_deposit_account,
            Some(params.general.owner.clone()),
            salts[0].clone(),
        )
        .await?;
    println!(
        "ICA deposit account instantiated: {}",
        ica_deposit_account_address
    );

    let mars_deposit_account = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![mars_lending_library_address.clone()],
    };
    let mars_deposit_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "mars_deposit".to_string(),
            mars_deposit_account,
            Some(params.general.owner.clone()),
            salts[1].clone(),
        )
        .await?;
    println!(
        "Mars deposit account instantiated: {}",
        mars_deposit_account_address
    );

    let supervault_deposit_account = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![supervaults_lper_library_address.clone()],
    };
    let supervault_deposit_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "supervault_deposit".to_string(),
            supervault_deposit_account,
            Some(params.general.owner.clone()),
            salts[2].clone(),
        )
        .await?;
    println!(
        "Supervault deposit account instantiated: {}",
        supervault_deposit_account_address
    );

    let settlement_account = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![clearing_queue_library_address.clone()],
    };
    let settlement_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "settlement".to_string(),
            settlement_account,
            Some(params.general.owner.clone()),
            salts[3].clone(),
        )
        .await?;
    println!(
        "Settlement account instantiated: {}",
        settlement_account_address
    );

    let denoms = NeutronDenoms {
        deposit_token: params.program.deposit_token_on_neutron_denom,
        ntrn: "untrn".to_string(),
        supervault_lp: params.program.supervault_lp_denom.clone(),
    };

    let accounts = NeutronAccounts {
        deposit: ica_deposit_account_address,
        mars_deposit: mars_deposit_account_address,
        supervault_deposit: supervault_deposit_account_address,
        settlement: settlement_account_address,
    };

    let libraries = NeutronLibraries {
        deposit_forwarder: deposit_forwarder_library_address,
        mars_lending: mars_lending_library_address,
        supervault_lper: supervaults_lper_library_address,
        clearing_queue: clearing_queue_library_address,
    };

    let neutron_cfg = NeutronStrategyConfig {
        grpc_url: params.general.grpc_url.clone(),
        grpc_port: params.general.grpc_port.clone(),
        chain_id: params.general.chain_id.clone(),
        mars_pool: "mars pool".to_string(),
        supervault: params.program.supervault.clone(),
        denoms,
        accounts,
        libraries,
        min_ibc_fee: Uint128::one(),
        authorizations: authorization_address,
        processor: processor_address,
    };

    println!("Neutron Strategy Config created successfully");

    // Save the Neutron Strategy Config to a toml file
    let neutron_cfg_toml =
        toml::to_string(&neutron_cfg).expect("Failed to serialize Neutron Strategy Config");
    fs::write(
        current_dir.join("deploy/src/neutron_strategy_config.toml"),
        neutron_cfg_toml,
    )
    .expect("Failed to write Neutron Strategy Config to file");

    Ok(())
}
