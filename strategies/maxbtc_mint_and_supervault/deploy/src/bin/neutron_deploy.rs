use std::{collections::BTreeMap, env, fs, time::SystemTime};

use cosmwasm_std::{Decimal, Uint64, Uint128};
use maxbtc_mint_and_supervault_deploy::{INPUTS_DIR, OUTPUTS_DIR};
use maxbtc_mint_and_supervault_types::{
    gaia_config::GaiaStrategyConfig,
    neutron_config::{
        NeutronAccounts, NeutronCoprocessorAppIds, NeutronDenoms, NeutronLibraries,
        NeutronStrategyConfig,
    },
};
use packages::{
    contracts::{PATH_NEUTRON_CODE_IDS, UploadedContracts},
    types::inputs::{ChainClientInputs, ClearingQueueCoprocessorApp},
    verification::VALENCE_NEUTRON_VERIFICATION_GATEWAY,
};
use serde::Deserialize;
use valence_clearing_queue_supervaults::msg::SupervaultSettlementInfo;
use valence_domain_clients::{
    clients::neutron::NeutronClient,
    cosmos::{grpc_client::GrpcSigningClient, wasm_client::WasmClient},
};
use valence_library_utils::LibraryAccountType;

#[derive(Deserialize, Debug)]
struct Parameters {
    general: General,
    ica: Ica,
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
    maxbtc_contract: String,
    maxbtc_denom: String,
    supervault_contract: String,
    supervault_asset1: String,
    supervault_asset2: String,
    supervault_lp_denom: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let mnemonic = env::var("MNEMONIC").expect("mnemonic must be provided");

    let current_dir = env::current_dir()?;

    println!("{}", format!("{INPUTS_DIR}/neutron.toml"));

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
    let code_id_ica_ibc_transfer_library =
        *uploaded_contracts.code_ids.get("ica_ibc_transfer").unwrap();
    let code_id_clearing_queue = *uploaded_contracts
        .code_ids
        .get("clearing_queue_supervaults")
        .unwrap();
    let code_id_base_account = *uploaded_contracts.code_ids.get("base_account").unwrap();
    let code_id_interchain_account = *uploaded_contracts
        .code_ids
        .get("interchain_account")
        .unwrap();
    let code_id_maxbtc_issuer = *uploaded_contracts.code_ids.get("maxbtc_issuer").unwrap();
    let code_id_supervaults_lper = *uploaded_contracts.code_ids.get("supervaults_lper").unwrap();

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

    // Predict all base accounts. We need 3:
    // 1. ICA Deposit Account (receives from Gaia)
    // 2. Supervault Deposit Account (receives minted maxBTC, deposits to supervault)
    // 3. Settlement Account (receives LP tokens from supervault)
    let mut salts = vec![];
    let mut predicted_base_accounts = vec![];
    for i in 0..3 {
        let salt = hex::encode(format!("{salt_raw}{i}").as_bytes());
        salts.push(salt.clone());
        let predicted_base_account_address = neutron_client
            .predict_instantiate2_addr(code_id_base_account, salt.clone(), my_address.clone())
            .await?
            .address;
        println!("Predicted base account address {i}: {predicted_base_account_address}");
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
        receiver: predicted_base_accounts[0].clone(), // Sends to ICA Deposit Account
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
        owner: processor_address.clone(),
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
    println!("ICA IBC Transfer library instantiated: {ica_ibc_transfer_library_address}");

    // Instantiate the maxBTC issuer library
    let maxbtc_issuer_config = valence_maxbtc_issuer::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[0].clone()), // Input is ICA Deposit Account
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[1].clone()),
        maxbtc_issuer_addr: params.program.maxbtc_contract.clone(),
        btc_denom: params.program.deposit_token_on_neutron_denom.clone(),
    };

    let instantiate_maxbtc_issuer_msg =
        valence_library_utils::msg::InstantiateMsg::<valence_maxbtc_issuer::msg::LibraryConfig> {
            owner: processor_address.clone(),
            processor: processor_address.clone(),
            config: maxbtc_issuer_config,
        };
    let maxbtc_issuer_library_address = neutron_client
        .instantiate(
            code_id_maxbtc_issuer,
            "maxbtc_issuer".to_string(),
            instantiate_maxbtc_issuer_msg,
            None,
        )
        .await?;
    println!("MaxBTC Issuer library instantiated: {maxbtc_issuer_library_address}");

    // Instantiate supervaults lper library
    let supervaults_lper_config = valence_supervaults_lper::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[1].clone()), // Input is Supervault Deposit Account
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[2].clone()), // Output is final Settlement Account
        vault_addr: params.program.supervault_contract.clone(),
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

    // Instantiate the clearing queue library with supervault info
    let clearing_config = valence_clearing_queue_supervaults::msg::LibraryConfig {
        settlement_acc_addr: LibraryAccountType::Addr(predicted_base_accounts[2].clone()), // Final settlement account
        denom: params.program.maxbtc_denom.clone(),
        latest_id: None,
        mars_settlement_ratio: Decimal::zero(), // No Mars lending
        supervaults_settlement_info: vec![SupervaultSettlementInfo {
            supervault_addr: params.program.supervault_contract.clone(),
            supervault_sender: predicted_base_accounts[1].clone(), // Sender is the Supervault Deposit Account
            settlement_ratio: Decimal::one(),                      // 100% to this supervault
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
    println!("Valence ICA instantiated: {valence_ica_address}");

    // Now the rest of the accounts
    let ica_deposit_account_msg = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![maxbtc_issuer_library_address.clone()],
    };
    let ica_deposit_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "ica_deposit".to_string(),
            ica_deposit_account_msg,
            Some(params.general.owner.clone()),
            salts[0].clone(),
        )
        .await?;
    println!("ICA deposit account instantiated: {ica_deposit_account_address}");

    // Instantiate the new intermediate account for supervault deposits
    let supervault_deposit_account_msg = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![supervaults_lper_library_address.clone()],
    };
    let supervault_deposit_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "supervault_deposit".to_string(),
            supervault_deposit_account_msg,
            Some(params.general.owner.clone()),
            salts[1].clone(),
        )
        .await?;
    println!("Supervault deposit account instantiated: {supervault_deposit_account_address}");

    let settlement_account_msg = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![clearing_queue_library_address.clone()],
    };
    let settlement_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "settlement".to_string(),
            settlement_account_msg,
            Some(params.general.owner.clone()),
            salts[2].clone(),
        )
        .await?;
    println!("Settlement account instantiated: {settlement_account_address}");

    // Update Denoms, Accounts, and Libraries for the final config
    let denoms = NeutronDenoms {
        deposit_token: params.program.deposit_token_on_neutron_denom,
        ntrn: "untrn".to_string(),
        maxbtc: params.program.maxbtc_denom.clone(),
        supervault_lp: params.program.supervault_lp_denom,
    };

    let accounts = NeutronAccounts {
        gaia_ica: valence_ica_address.clone(),
        ica_deposit: ica_deposit_account_address,
        supervault_deposit: supervault_deposit_account_address,
        settlement: settlement_account_address,
    };

    let libraries = NeutronLibraries {
        maxbtc_issuer: maxbtc_issuer_library_address,
        clearing_queue: clearing_queue_library_address,
        ica_transfer_gaia: ica_ibc_transfer_library_address,
        supervault_lper: supervaults_lper_library_address,
    };

    let coprocessor_app_ids = NeutronCoprocessorAppIds {
        clearing_queue: params.coprocessor_app.clearing_queue_coprocessor_app_id,
    };

    let neutron_cfg = NeutronStrategyConfig {
        grpc_url: params.general.grpc_url.clone(),
        grpc_port: params.general.grpc_port.clone(),
        chain_id: params.general.chain_id.clone(),
        maxbtc_contract: params.program.maxbtc_contract.clone(),
        supervault_contract: params.program.supervault_contract,
        denoms,
        accounts,
        libraries,
        authorizations: authorization_address,
        processor: processor_address,
        coprocessor_app_ids,
    };

    println!("Neutron Strategy Config created successfully, saving to {}", format!("{OUTPUTS_DIR}/neutron_strategy_config.toml"));

    // Save the Neutron Strategy Config to a toml file
    let neutron_cfg_toml =
        toml::to_string(&neutron_cfg).expect("Failed to serialize Neutron Strategy Config");
    fs::write(
        current_dir.join(format!("{OUTPUTS_DIR}/neutron_strategy_config.toml")),
        neutron_cfg_toml,
    )
    .expect("Failed to write Neutron Strategy Config to file");

    // Last thing we will do is register the ICA on the valence ICA
    let register_ica_msg = valence_account_utils::ica::ExecuteMsg::RegisterIca {};
    neutron_client
        .execute_wasm(
            &valence_ica_address,
            register_ica_msg,
            vec![cosmrs::Coin::new(1_000_000u128, "untrn").unwrap()],
            None,
        )
        .await?;

    println!("Registering ICA...");

    // Let's wait enough time for the transaction to succeed and the ICA to be registered
    tokio::time::sleep(std::time::Duration::from_secs(60)).await;

    // Let's query now to get the ICA address
    let query_ica = valence_account_utils::ica::QueryMsg::IcaState {};
    let ica_state: valence_account_utils::ica::IcaState = neutron_client
        .query_contract_state(&valence_ica_address, query_ica)
        .await?;

    let ica_address = match ica_state {
        valence_account_utils::ica::IcaState::Created(ica_information) => ica_information.address,
        _ => {
            panic!("ICA creation failed!, state: {ica_state:?}");
        }
    };
    println!("ICA address: {ica_address}");

    let gaia_inputs = fs::read_to_string(current_dir.join(format!("{INPUTS_DIR}/gaia.toml")))
        .expect("Failed to read file");

    let gaia_inputs: ChainClientInputs =
        toml::from_str(&gaia_inputs).expect("Failed to parse gaia toml inputs");

    let gaia_cfg = GaiaStrategyConfig {
        grpc_url: gaia_inputs.grpc_url,
        grpc_port: gaia_inputs.grpc_port,
        chain_id: gaia_inputs.chain_id,
        chain_denom: gaia_inputs.chain_denom,
        deposit_denom: params.ica.deposit_token_on_hub_denom.clone(),
        ica_address,
    };

    // Write the Gaia strategy config to a file
    let gaia_cfg_path = current_dir.join(format!("{OUTPUTS_DIR}/gaia_strategy_config.toml"));
    fs::write(
        gaia_cfg_path,
        toml::to_string(&gaia_cfg).expect("Failed to serialize Gaia strategy config"),
    )
    .expect("Failed to write Gaia strategy config to file");

    Ok(())
}
