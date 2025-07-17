use std::{
    collections::{BTreeMap, HashMap},
    env, fs,
    str::FromStr,
    time::SystemTime,
};

use cosmwasm_std::{Decimal, Uint64, Uint128};
use packages::{
    contracts::PATH_NEUTRON_CODE_IDS, verification::VALENCE_NEUTRON_VERIFICATION_GATEWAY,
};
use serde::Deserialize;
use valence_clearing_queue_supervaults::msg::SupervaultSettlementInfo;
use valence_domain_clients::{
    clients::neutron::NeutronClient,
    cosmos::{grpc_client::GrpcSigningClient, wasm_client::WasmClient},
};
use wbtc_deploy::{INPUTS_DIR, OUTPUTS_DIR};
use wbtc_types::{
    gaia_config::GaiaStrategyConfig,
    neutron_config::{
        NeutronAccounts, NeutronCoprocessorAppIds, NeutronDenoms, NeutronLibraries,
        NeutronStrategyConfig,
    },
};

use valence_dynamic_ratio_query_provider::msg::DenomSplitMap;
use valence_forwarder_library::msg::ForwardingConstraints;
use valence_library_utils::{LibraryAccountType, denoms::UncheckedDenom};
use valence_splitter_library::msg::{UncheckedSplitAmount, UncheckedSplitConfig};

#[derive(Deserialize, Debug)]
struct UploadedContracts {
    code_ids: HashMap<String, u64>,
}

#[derive(Deserialize, Debug)]
struct Parameters {
    general: General,
    ica: Ica,
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
    fbtc_supervault: String,
    fbtc_supervault_asset1: String,
    fbtc_supervault_asset2: String,
    fbtc_supervault_lp_denom: String,
    lbtc_supervault: String,
    lbtc_supervault_asset1: String,
    lbtc_supervault_asset2: String,
    lbtc_supervault_lp_denom: String,
    solvbtc_supervault: String,
    solvbtc_supervault_asset1: String,
    solvbtc_supervault_asset2: String,
    solvbtc_supervault_lp_denom: String,
    ebtc_supervault: String,
    ebtc_supervault_asset1: String,
    ebtc_supervault_asset2: String,
    ebtc_supervault_lp_denom: String,
    pumpbtc_supervault: String,
    pumpbtc_supervault_asset1: String,
    pumpbtc_supervault_asset2: String,
    pumpbtc_supervault_lp_denom: String,
    bedrockbtc_supervault: String,
    bedrockbtc_supervault_asset1: String,
    bedrockbtc_supervault_asset2: String,
    bedrockbtc_supervault_lp_denom: String,
    initial_split_mars_ratio: String,
    initial_split_fbtc_ratio: String,
    initial_split_lbtc_ratio: String,
    initial_split_solvbtc_ratio: String,
    initial_split_ebtc_ratio: String,
    initial_split_pumpbtc_ratio: String,
    initial_split_bedrockbtc_ratio: String,
    mars_settlement_ratio: String,
    fbtc_settlement_ratio_percentage: u64,
    lbtc_settlement_ratio_percentage: u64,
    solvbtc_settlement_ratio_percentage: u64,
    ebtc_settlement_ratio_percentage: u64,
    pumpbtc_settlement_ratio_percentage: u64,
    bedrockbtc_settlement_ratio_percentage: u64,
    fbtc_denom: String,
    lbtc_denom: String,
    solvbtc_denom: String,
    ebtc_denom: String,
    pumpbtc_denom: String,
    bedrockbtc_denom: String,
}

#[derive(Deserialize, Debug)]
struct CoprocessorApp {
    clearing_queue_coprocessor_app_id: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
    let code_id_ica_ibc_transfer_library =
        *uploaded_contracts.code_ids.get("ica_ibc_transfer").unwrap();
    let code_id_splitter_library = *uploaded_contracts.code_ids.get("splitter_library").unwrap();
    let code_id_forwarder_library = *uploaded_contracts
        .code_ids
        .get("forwarder_library")
        .unwrap();
    let code_id_mars_lending = *uploaded_contracts.code_ids.get("mars_lending").unwrap();
    let code_id_supervaults_lper = *uploaded_contracts.code_ids.get("supervaults_lper").unwrap();
    let code_id_supervaults_withdrawer = *uploaded_contracts
        .code_ids
        .get("supervaults_withdrawer")
        .unwrap();
    let code_id_clearing_queue = *uploaded_contracts
        .code_ids
        .get("clearing_queue_supervaults")
        .unwrap();
    let code_id_base_account = *uploaded_contracts.code_ids.get("base_account").unwrap();
    let code_id_interchain_account = *uploaded_contracts
        .code_ids
        .get("interchain_account")
        .unwrap();
    let code_id_dynamic_ratio_query_provider = *uploaded_contracts
        .code_ids
        .get("dynamic_ratio_query_provider")
        .unwrap();
    let code_id_maxbtc_issuer = *uploaded_contracts.code_ids.get("maxbtc_issuer").unwrap();

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

    // Predict all base accounts, we are going to store all salts for them as well. In total we need 4
    let mut salts = vec![];
    let mut predicted_base_accounts = vec![];
    // We now will have 7 supervault deposit accounts, including the maxBTC-BTC vault one which will come later when the vault launches
    for i in 0..10 {
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

    // Instantiate the Dynamic ratio query provider library
    let receiver_to_split_perc = HashMap::from([
        (
            predicted_base_accounts[1].clone(), // Mars deposit account
            Decimal::from_str(&params.program.initial_split_mars_ratio).unwrap(),
        ),
        (
            predicted_base_accounts[2].clone(), // FBTC WBTC supervault deposit account
            Decimal::from_str(&params.program.initial_split_fbtc_ratio).unwrap(),
        ),
        (
            predicted_base_accounts[3].clone(), // LBTC WBTC supervault deposit account
            Decimal::from_str(&params.program.initial_split_lbtc_ratio).unwrap(),
        ),
        (
            predicted_base_accounts[4].clone(), // SolvBTC WBTC supervault deposit account
            Decimal::from_str(&params.program.initial_split_solvbtc_ratio).unwrap(),
        ),
        (
            predicted_base_accounts[5].clone(), // eBTC WBTC supervault deposit account
            Decimal::from_str(&params.program.initial_split_ebtc_ratio).unwrap(),
        ),
        (
            predicted_base_accounts[6].clone(), // PumpBTC WBTC supervault deposit account
            Decimal::from_str(&params.program.initial_split_pumpbtc_ratio).unwrap(),
        ),
        (
            predicted_base_accounts[7].clone(), // bedrockbtc WBTC supervault deposit account
            Decimal::from_str(&params.program.initial_split_bedrockbtc_ratio).unwrap(),
        ),
    ]);

    let denom_to_splits = HashMap::from([(
        params.program.deposit_token_on_neutron_denom.clone(),
        receiver_to_split_perc,
    )]);

    let dynamic_ratio_query_provider_instantiate_msg =
        valence_dynamic_ratio_query_provider::msg::InstantiateMsg {
            admin: params.general.owner.clone(),
            split_cfg: DenomSplitMap {
                split_cfg: denom_to_splits,
            },
        };

    let dynamic_ratio_query_provider_address = neutron_client
        .instantiate(
            code_id_dynamic_ratio_query_provider,
            "dynamic_ratio_query_provider".to_string(),
            dynamic_ratio_query_provider_instantiate_msg,
            None,
        )
        .await?;
    println!(
        "Dynamic ratio query provider library instantiated: {dynamic_ratio_query_provider_address}",
    );

    // Instantiate the deposit splitter library
    // This library will split to the Mars deposit account and the Supervault deposit accounts
    let deposit_splitter_config = valence_splitter_library::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[0].clone()),
        splits: vec![
            valence_splitter_library::msg::UncheckedSplitConfig {
                denom: UncheckedDenom::Native(
                    params.program.deposit_token_on_neutron_denom.clone(),
                ),
                account: LibraryAccountType::Addr(predicted_base_accounts[1].clone()), // Mars deposit account
                amount: UncheckedSplitAmount::DynamicRatio {
                    contract_addr: dynamic_ratio_query_provider_address.clone(),
                    params: predicted_base_accounts[1].clone(),
                },
            },
            valence_splitter_library::msg::UncheckedSplitConfig {
                denom: UncheckedDenom::Native(
                    params.program.deposit_token_on_neutron_denom.clone(),
                ),
                account: LibraryAccountType::Addr(predicted_base_accounts[2].clone()), // FBTC WBTC supervault deposit account
                amount: UncheckedSplitAmount::DynamicRatio {
                    contract_addr: dynamic_ratio_query_provider_address.clone(),
                    params: predicted_base_accounts[2].clone(),
                },
            },
            valence_splitter_library::msg::UncheckedSplitConfig {
                denom: UncheckedDenom::Native(
                    params.program.deposit_token_on_neutron_denom.clone(),
                ),
                account: LibraryAccountType::Addr(predicted_base_accounts[3].clone()), // LBTC WBTC supervault deposit account
                amount: UncheckedSplitAmount::DynamicRatio {
                    contract_addr: dynamic_ratio_query_provider_address.clone(),
                    params: predicted_base_accounts[3].clone(),
                },
            },
            valence_splitter_library::msg::UncheckedSplitConfig {
                denom: UncheckedDenom::Native(
                    params.program.deposit_token_on_neutron_denom.clone(),
                ),
                account: LibraryAccountType::Addr(predicted_base_accounts[4].clone()), // SolvBTC WBTC supervault deposit account
                amount: UncheckedSplitAmount::DynamicRatio {
                    contract_addr: dynamic_ratio_query_provider_address.clone(),
                    params: predicted_base_accounts[4].clone(),
                },
            },
            valence_splitter_library::msg::UncheckedSplitConfig {
                denom: UncheckedDenom::Native(
                    params.program.deposit_token_on_neutron_denom.clone(),
                ),
                account: LibraryAccountType::Addr(predicted_base_accounts[5].clone()), // eBTC WBTC supervault deposit account
                amount: UncheckedSplitAmount::DynamicRatio {
                    contract_addr: dynamic_ratio_query_provider_address.clone(),
                    params: predicted_base_accounts[5].clone(),
                },
            },
            valence_splitter_library::msg::UncheckedSplitConfig {
                denom: UncheckedDenom::Native(
                    params.program.deposit_token_on_neutron_denom.clone(),
                ),
                account: LibraryAccountType::Addr(predicted_base_accounts[6].clone()), // PumpBTC WBTC supervault deposit account
                amount: UncheckedSplitAmount::DynamicRatio {
                    contract_addr: dynamic_ratio_query_provider_address.clone(),
                    params: predicted_base_accounts[6].clone(),
                },
            },
            valence_splitter_library::msg::UncheckedSplitConfig {
                denom: UncheckedDenom::Native(
                    params.program.deposit_token_on_neutron_denom.clone(),
                ),
                account: LibraryAccountType::Addr(predicted_base_accounts[7].clone()), // bedrockbtc WBTC supervault deposit account
                amount: UncheckedSplitAmount::DynamicRatio {
                    contract_addr: dynamic_ratio_query_provider_address.clone(),
                    params: predicted_base_accounts[7].clone(),
                },
            },
        ],
    };

    let instantiate_deposit_splitter_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_splitter_library::msg::LibraryConfig,
    > {
        owner: processor_address.clone(),
        processor: processor_address.clone(),
        config: deposit_splitter_config,
    };

    let deposit_splitter_library_address = neutron_client
        .instantiate(
            code_id_splitter_library,
            "deposit_splitter".to_string(),
            instantiate_deposit_splitter_msg,
            None,
        )
        .await?;
    println!("Deposit splitter library instantiated: {deposit_splitter_library_address}");

    // Instantiate the Mars lending library
    // In Phase 1 the output account is the settlement account and in phase 2 this will be initial deposit account
    let mars_lending_config = valence_mars_lending::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[1].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        credit_manager_addr: params.program.mars_credit_manager.clone(),
        denom: params.program.deposit_token_on_neutron_denom.clone(),
    };

    let instantiate_mars_lending_msg =
        valence_library_utils::msg::InstantiateMsg::<valence_mars_lending::msg::LibraryConfig> {
            owner: processor_address.clone(),
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
    println!("Mars lending library instantiated: {mars_lending_library_address}");

    // Instantiate all supervaults lper library
    let supervaults_fbtc_lper_config = valence_supervaults_lper::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[2].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        vault_addr: params.program.fbtc_supervault.clone(),
        lp_config: valence_supervaults_lper::msg::LiquidityProviderConfig {
            asset_data: valence_library_utils::liquidity_utils::AssetData {
                asset1: params.program.fbtc_supervault_asset1.clone(),
                asset2: params.program.fbtc_supervault_asset2.clone(),
            },
            lp_denom: params.program.fbtc_supervault_lp_denom.clone(),
        },
    };

    let instantiate_fbtc_supervaults_lper_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_supervaults_lper::msg::LibraryConfig,
    > {
        owner: processor_address.clone(),
        processor: processor_address.clone(),
        config: supervaults_fbtc_lper_config,
    };
    let fbtc_supervaults_lper_library_address = neutron_client
        .instantiate(
            code_id_supervaults_lper,
            "fbtc_supervault_lper".to_string(),
            instantiate_fbtc_supervaults_lper_msg,
            None,
        )
        .await?;
    println!(
        "FBTC-WBTC Supervaults lper library instantiated: {fbtc_supervaults_lper_library_address}",
    );

    let lbtc_supervaults_lper_config = valence_supervaults_lper::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[3].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        vault_addr: params.program.lbtc_supervault.clone(),
        lp_config: valence_supervaults_lper::msg::LiquidityProviderConfig {
            asset_data: valence_library_utils::liquidity_utils::AssetData {
                asset1: params.program.lbtc_supervault_asset1.clone(),
                asset2: params.program.lbtc_supervault_asset2.clone(),
            },
            lp_denom: params.program.lbtc_supervault_lp_denom.clone(),
        },
    };

    let instantiate_lbtc_supervaults_lper_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_supervaults_lper::msg::LibraryConfig,
    > {
        owner: processor_address.clone(),
        processor: processor_address.clone(),
        config: lbtc_supervaults_lper_config,
    };

    let lbtc_supervaults_lper_library_address = neutron_client
        .instantiate(
            code_id_supervaults_lper,
            "lbtc_supervault_lper".to_string(),
            instantiate_lbtc_supervaults_lper_msg,
            None,
        )
        .await?;

    println!(
        "LBTC-WBTC Supervaults lper library instantiated: {lbtc_supervaults_lper_library_address}",
    );

    let solvbtc_supervaults_lper_config = valence_supervaults_lper::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[4].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        vault_addr: params.program.solvbtc_supervault.clone(),
        lp_config: valence_supervaults_lper::msg::LiquidityProviderConfig {
            asset_data: valence_library_utils::liquidity_utils::AssetData {
                asset1: params.program.solvbtc_supervault_asset1.clone(),
                asset2: params.program.solvbtc_supervault_asset2.clone(),
            },
            lp_denom: params.program.solvbtc_supervault_lp_denom.clone(),
        },
    };

    let instantiate_solvbtc_supervaults_lper_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_supervaults_lper::msg::LibraryConfig,
    > {
        owner: processor_address.clone(),
        processor: processor_address.clone(),
        config: solvbtc_supervaults_lper_config,
    };
    let solvbtc_supervaults_lper_library_address = neutron_client
        .instantiate(
            code_id_supervaults_lper,
            "solvbtc_supervault_lper".to_string(),
            instantiate_solvbtc_supervaults_lper_msg,
            None,
        )
        .await?;
    println!(
        "SolvBTC-WBTC Supervaults lper library instantiated: {solvbtc_supervaults_lper_library_address}",
    );

    let ebtc_supervaults_lper_config = valence_supervaults_lper::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[5].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        vault_addr: params.program.ebtc_supervault.clone(),
        lp_config: valence_supervaults_lper::msg::LiquidityProviderConfig {
            asset_data: valence_library_utils::liquidity_utils::AssetData {
                asset1: params.program.ebtc_supervault_asset1.clone(),
                asset2: params.program.ebtc_supervault_asset2.clone(),
            },
            lp_denom: params.program.ebtc_supervault_lp_denom.clone(),
        },
    };

    let instantiate_ebtc_supervaults_lper_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_supervaults_lper::msg::LibraryConfig,
    > {
        owner: processor_address.clone(),
        processor: processor_address.clone(),
        config: ebtc_supervaults_lper_config,
    };

    let ebtc_supervaults_lper_library_address = neutron_client
        .instantiate(
            code_id_supervaults_lper,
            "ebtc_supervault_lper".to_string(),
            instantiate_ebtc_supervaults_lper_msg,
            None,
        )
        .await?;
    println!(
        "eBTC-WBTC Supervaults lper library instantiated: {ebtc_supervaults_lper_library_address}",
    );

    let pumpbtc_supervaults_lper_config = valence_supervaults_lper::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[6].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        vault_addr: params.program.pumpbtc_supervault.clone(),
        lp_config: valence_supervaults_lper::msg::LiquidityProviderConfig {
            asset_data: valence_library_utils::liquidity_utils::AssetData {
                asset1: params.program.pumpbtc_supervault_asset1.clone(),
                asset2: params.program.pumpbtc_supervault_asset2.clone(),
            },
            lp_denom: params.program.pumpbtc_supervault_lp_denom.clone(),
        },
    };

    let instantiate_pumpbtc_supervaults_lper_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_supervaults_lper::msg::LibraryConfig,
    > {
        owner: processor_address.clone(),
        processor: processor_address.clone(),
        config: pumpbtc_supervaults_lper_config,
    };
    let pumpbtc_supervaults_lper_library_address = neutron_client
        .instantiate(
            code_id_supervaults_lper,
            "pumpbtc_supervault_lper".to_string(),
            instantiate_pumpbtc_supervaults_lper_msg,
            None,
        )
        .await?;
    println!(
        "PumpBTC-WBTC Supervaults lper library instantiated: {pumpbtc_supervaults_lper_library_address}",
    );

    let bedrockbtc_supervaults_lper_config = valence_supervaults_lper::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[7].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        vault_addr: params.program.bedrockbtc_supervault.clone(),
        lp_config: valence_supervaults_lper::msg::LiquidityProviderConfig {
            asset_data: valence_library_utils::liquidity_utils::AssetData {
                asset1: params.program.bedrockbtc_supervault_asset1.clone(),
                asset2: params.program.bedrockbtc_supervault_asset2.clone(),
            },
            lp_denom: params.program.bedrockbtc_supervault_lp_denom.clone(),
        },
    };
    let instantiate_bedrockbtc_supervaults_lper_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_supervaults_lper::msg::LibraryConfig,
    > {
        owner: processor_address.clone(),
        processor: processor_address.clone(),
        config: bedrockbtc_supervaults_lper_config,
    };
    let bedrockbtc_supervaults_lper_library_address = neutron_client
        .instantiate(
            code_id_supervaults_lper,
            "bedrockbtc_supervault_lper".to_string(),
            instantiate_bedrockbtc_supervaults_lper_msg,
            None,
        )
        .await?;
    println!(
        "bedrockbtc-WBTC Supervaults lper library instantiated: {bedrockbtc_supervaults_lper_library_address}",
    );

    // We'll also instantiate the maxBTC-BTC one even though we don't have the address yet,
    // but we will update it later, this way we can set up the authorizations for it already
    // We'll use the FBTC-WBTC supervault lper library address and parameters as placeholders
    let maxbtc_btc_supervaults_lper_config = valence_supervaults_lper::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[8].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        vault_addr: params.program.fbtc_supervault.clone(),
        lp_config: valence_supervaults_lper::msg::LiquidityProviderConfig {
            asset_data: valence_library_utils::liquidity_utils::AssetData {
                asset1: params.program.fbtc_supervault_asset1.clone(),
                asset2: params.program.fbtc_supervault_asset2.clone(),
            },
            lp_denom: params.program.fbtc_supervault_lp_denom.clone(),
        },
    };

    let instantiate_maxbtc_btc_supervaults_lper_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_supervaults_lper::msg::LibraryConfig,
    > {
        owner: processor_address.clone(),
        processor: processor_address.clone(),
        config: maxbtc_btc_supervaults_lper_config,
    };
    let maxbtc_btc_supervaults_lper_library_address = neutron_client
        .instantiate(
            code_id_supervaults_lper,
            "maxbtc_btc_supervault_lper".to_string(),
            instantiate_maxbtc_btc_supervaults_lper_msg,
            None,
        )
        .await?;
    println!(
        "Placeholer MaxBTC-BTC Supervaults lper library instantiated: {maxbtc_btc_supervaults_lper_library_address}",
    );

    // Finally instantiate the clearing queue library
    let clearing_config = valence_clearing_queue_supervaults::msg::LibraryConfig {
        settlement_acc_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        denom: params.program.deposit_token_on_neutron_denom.clone(),
        latest_id: None,
        mars_settlement_ratio: Decimal::from_str(&params.program.mars_settlement_ratio).unwrap(),
        supervaults_settlement_info: vec![
            SupervaultSettlementInfo {
                supervault_addr: params.program.fbtc_supervault.clone(),
                supervault_sender: predicted_base_accounts[2].clone(), // Input account of supervaults lper library
                settlement_ratio: Decimal::percent(params.program.fbtc_settlement_ratio_percentage),
            },
            SupervaultSettlementInfo {
                supervault_addr: params.program.lbtc_supervault.clone(),
                supervault_sender: predicted_base_accounts[3].clone(),
                settlement_ratio: Decimal::percent(params.program.lbtc_settlement_ratio_percentage),
            },
            SupervaultSettlementInfo {
                supervault_addr: params.program.solvbtc_supervault.clone(),
                supervault_sender: predicted_base_accounts[4].clone(),
                settlement_ratio: Decimal::percent(
                    params.program.solvbtc_settlement_ratio_percentage,
                ),
            },
            SupervaultSettlementInfo {
                supervault_addr: params.program.ebtc_supervault.clone(),
                supervault_sender: predicted_base_accounts[5].clone(),
                settlement_ratio: Decimal::percent(params.program.ebtc_settlement_ratio_percentage),
            },
            SupervaultSettlementInfo {
                supervault_addr: params.program.pumpbtc_supervault.clone(),
                supervault_sender: predicted_base_accounts[6].clone(),
                settlement_ratio: Decimal::percent(
                    params.program.pumpbtc_settlement_ratio_percentage,
                ),
            },
            SupervaultSettlementInfo {
                supervault_addr: params.program.bedrockbtc_supervault.clone(),
                supervault_sender: predicted_base_accounts[7].clone(),
                settlement_ratio: Decimal::percent(
                    params.program.bedrockbtc_settlement_ratio_percentage,
                ),
            },
        ],
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

    ////////// Instantiate and set up everything we need for the phase shift. The authorization that will eventually //////////
    ////////// update the config and execute these libraries will be for the program owner, in this case the Neutron //////////
    ////////// program DAO set up for this.                                                                          //////////

    // 1. Instantiate all the supervaults withdrawers that will withdraw the LP tokens from the supervault and deposit the deposit token + the other pair
    // in the settlement account.
    let fbtc_supervault_withdrawer_config = valence_supervaults_withdrawer::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()), // Both input and output are settlement account
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        vault_addr: params.program.fbtc_supervault.clone(),
        lw_config: valence_supervaults_withdrawer::msg::LiquidityWithdrawerConfig {
            asset_data: valence_library_utils::liquidity_utils::AssetData {
                asset1: params.program.fbtc_supervault_asset1.clone(),
                asset2: params.program.fbtc_supervault_asset2.clone(),
            },
            lp_denom: params.program.fbtc_supervault_lp_denom.clone(),
        },
    };

    let instantiate_fbtc_supervaults_withdrawer_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_supervaults_withdrawer::msg::LibraryConfig,
    > {
        owner: processor_address.clone(),
        processor: processor_address.clone(),
        config: fbtc_supervault_withdrawer_config,
    };
    let fbtc_supervault_withdrawer_library_address = neutron_client
        .instantiate(
            code_id_supervaults_withdrawer,
            "supervaults_withdrawer".to_string(),
            instantiate_fbtc_supervaults_withdrawer_msg,
            None,
        )
        .await?;
    println!(
        "FBTC-WBTC Supervaults withdrawer library instantiated: {fbtc_supervault_withdrawer_library_address}",
    );

    let lbtc_supervault_withdrawer_config = valence_supervaults_withdrawer::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        vault_addr: params.program.lbtc_supervault.clone(),
        lw_config: valence_supervaults_withdrawer::msg::LiquidityWithdrawerConfig {
            asset_data: valence_library_utils::liquidity_utils::AssetData {
                asset1: params.program.lbtc_supervault_asset1.clone(),
                asset2: params.program.lbtc_supervault_asset2.clone(),
            },
            lp_denom: params.program.lbtc_supervault_lp_denom.clone(),
        },
    };

    let instantiate_lbtc_supervaults_withdrawer_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_supervaults_withdrawer::msg::LibraryConfig,
    > {
        owner: processor_address.clone(),
        processor: processor_address.clone(),
        config: lbtc_supervault_withdrawer_config,
    };
    let lbtc_supervault_withdrawer_library_address = neutron_client
        .instantiate(
            code_id_supervaults_withdrawer,
            "lbtc_supervault_withdrawer".to_string(),
            instantiate_lbtc_supervaults_withdrawer_msg,
            None,
        )
        .await?;
    println!(
        "LBTC-WBTC Supervaults withdrawer library instantiated: {lbtc_supervault_withdrawer_library_address}",
    );

    let solvbtc_supervault_withdrawer_config = valence_supervaults_withdrawer::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        vault_addr: params.program.solvbtc_supervault.clone(),
        lw_config: valence_supervaults_withdrawer::msg::LiquidityWithdrawerConfig {
            asset_data: valence_library_utils::liquidity_utils::AssetData {
                asset1: params.program.solvbtc_supervault_asset1.clone(),
                asset2: params.program.solvbtc_supervault_asset2.clone(),
            },
            lp_denom: params.program.solvbtc_supervault_lp_denom.clone(),
        },
    };
    let instantiate_solvbtc_supervaults_withdrawer_msg =
        valence_library_utils::msg::InstantiateMsg::<
            valence_supervaults_withdrawer::msg::LibraryConfig,
        > {
            owner: processor_address.clone(),
            processor: processor_address.clone(),
            config: solvbtc_supervault_withdrawer_config,
        };
    let solvbtc_supervault_withdrawer_library_address = neutron_client
        .instantiate(
            code_id_supervaults_withdrawer,
            "solvbtc_supervault_withdrawer".to_string(),
            instantiate_solvbtc_supervaults_withdrawer_msg,
            None,
        )
        .await?;
    println!(
        "SolvBTC-WBTC Supervaults withdrawer library instantiated: {solvbtc_supervault_withdrawer_library_address}"
    );

    let ebtc_supervault_withdrawer_config = valence_supervaults_withdrawer::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        vault_addr: params.program.ebtc_supervault.clone(),
        lw_config: valence_supervaults_withdrawer::msg::LiquidityWithdrawerConfig {
            asset_data: valence_library_utils::liquidity_utils::AssetData {
                asset1: params.program.ebtc_supervault_asset1.clone(),
                asset2: params.program.ebtc_supervault_asset2.clone(),
            },
            lp_denom: params.program.ebtc_supervault_lp_denom.clone(),
        },
    };
    let instantiate_ebtc_supervaults_withdrawer_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_supervaults_withdrawer::msg::LibraryConfig,
    > {
        owner: processor_address.clone(),
        processor: processor_address.clone(),
        config: ebtc_supervault_withdrawer_config,
    };
    let ebtc_supervault_withdrawer_library_address = neutron_client
        .instantiate(
            code_id_supervaults_withdrawer,
            "ebtc_supervault_withdrawer".to_string(),
            instantiate_ebtc_supervaults_withdrawer_msg,
            None,
        )
        .await?;
    println!(
        "eBTC-WBTC Supervaults withdrawer library instantiated: {ebtc_supervault_withdrawer_library_address}"
    );

    let pumpbtc_supervault_withdrawer_config = valence_supervaults_withdrawer::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        vault_addr: params.program.pumpbtc_supervault.clone(),
        lw_config: valence_supervaults_withdrawer::msg::LiquidityWithdrawerConfig {
            asset_data: valence_library_utils::liquidity_utils::AssetData {
                asset1: params.program.pumpbtc_supervault_asset1.clone(),
                asset2: params.program.pumpbtc_supervault_asset2.clone(),
            },
            lp_denom: params.program.pumpbtc_supervault_lp_denom.clone(),
        },
    };
    let instantiate_pumpbtc_supervaults_withdrawer_msg =
        valence_library_utils::msg::InstantiateMsg::<
            valence_supervaults_withdrawer::msg::LibraryConfig,
        > {
            owner: processor_address.clone(),
            processor: processor_address.clone(),
            config: pumpbtc_supervault_withdrawer_config,
        };
    let pumpbtc_supervault_withdrawer_library_address = neutron_client
        .instantiate(
            code_id_supervaults_withdrawer,
            "pumpbtc_supervault_withdrawer".to_string(),
            instantiate_pumpbtc_supervaults_withdrawer_msg,
            None,
        )
        .await?;
    println!(
        "PumpBTC-WBTC Supervaults withdrawer library instantiated: {pumpbtc_supervault_withdrawer_library_address}"
    );

    let bedrockbtc_supervault_withdrawer_config =
        valence_supervaults_withdrawer::msg::LibraryConfig {
            input_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
            output_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
            vault_addr: params.program.bedrockbtc_supervault.clone(),
            lw_config: valence_supervaults_withdrawer::msg::LiquidityWithdrawerConfig {
                asset_data: valence_library_utils::liquidity_utils::AssetData {
                    asset1: params.program.bedrockbtc_supervault_asset1.clone(),
                    asset2: params.program.bedrockbtc_supervault_asset2.clone(),
                },
                lp_denom: params.program.bedrockbtc_supervault_lp_denom.clone(),
            },
        };
    let instantiate_bedrockbtc_supervaults_withdrawer_msg =
        valence_library_utils::msg::InstantiateMsg::<
            valence_supervaults_withdrawer::msg::LibraryConfig,
        > {
            owner: processor_address.clone(),
            processor: processor_address.clone(),
            config: bedrockbtc_supervault_withdrawer_config,
        };
    let bedrockbtc_supervault_withdrawer_library_address = neutron_client
        .instantiate(
            code_id_supervaults_withdrawer,
            "bedrockbtc_supervault_withdrawer".to_string(),
            instantiate_bedrockbtc_supervaults_withdrawer_msg,
            None,
        )
        .await?;
    println!(
        "bedrockbtc-WBTC Supervaults withdrawer library instantiated: {bedrockbtc_supervault_withdrawer_library_address}"
    );

    // 2. Instantiate the maxbtc issuer that will issue the maxbtc token depositing the counterparty of the deposit token in the vault.
    // The output address will be the deposit account for the supervault
    let supervault_other_asset =
        if params.program.fbtc_supervault_asset1 == params.program.deposit_token_on_neutron_denom {
            params.program.fbtc_supervault_asset2.clone()
        } else {
            params.program.fbtc_supervault_asset1.clone()
        };

    let maxbtc_issuer_config = valence_maxbtc_issuer::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[8].clone()), // Deposit address of the maxBTC supervault
        maxbtc_issuer_addr: params.general.owner.clone(), // We are going to put a dummy address here (the owner for example) because this will be eventually updated
        btc_denom: supervault_other_asset, // The counterparty asset of the vault (WBTC)
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
    println!("MaxBTC issuer library instantiated: {maxbtc_issuer_library_address}");

    // 3. Instantiate the splitter library that will return all LSTs to their corresponding LST deposit accounts
    let splitter_config = valence_splitter_library::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()), // Settlement account
        splits: vec![
            UncheckedSplitConfig {
                denom: UncheckedDenom::Native(params.program.fbtc_denom.clone()),
                account: LibraryAccountType::Addr(predicted_base_accounts[2].clone()), // FBTC deposit account
                amount: UncheckedSplitAmount::FixedRatio(Decimal::one()), // 100% of FBTC
            },
            UncheckedSplitConfig {
                denom: UncheckedDenom::Native(params.program.lbtc_denom.clone()),
                account: LibraryAccountType::Addr(predicted_base_accounts[3].clone()), // LBTC deposit account
                amount: UncheckedSplitAmount::FixedRatio(Decimal::one()), // 100% of LBTC
            },
            UncheckedSplitConfig {
                denom: UncheckedDenom::Native(params.program.solvbtc_denom.clone()),
                account: LibraryAccountType::Addr(predicted_base_accounts[4].clone()), // SolvBTC deposit account
                amount: UncheckedSplitAmount::FixedRatio(Decimal::one()), // 100% of SolvBTC
            },
            UncheckedSplitConfig {
                denom: UncheckedDenom::Native(params.program.ebtc_denom.clone()),
                account: LibraryAccountType::Addr(predicted_base_accounts[5].clone()), // eBTC deposit account
                amount: UncheckedSplitAmount::FixedRatio(Decimal::one()), // 100% of eBTC
            },
            UncheckedSplitConfig {
                denom: UncheckedDenom::Native(params.program.pumpbtc_denom.clone()),
                account: LibraryAccountType::Addr(predicted_base_accounts[6].clone()), // PumpBTC deposit account
                amount: UncheckedSplitAmount::FixedRatio(Decimal::one()), // 100% of PumpBTC
            },
            UncheckedSplitConfig {
                denom: UncheckedDenom::Native(params.program.bedrockbtc_denom.clone()),
                account: LibraryAccountType::Addr(predicted_base_accounts[7].clone()), // bedrockbtc deposit account
                amount: UncheckedSplitAmount::FixedRatio(Decimal::one()), // 100% of bedrockbtc
            },
        ],
    };

    let instantiate_splitter_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_splitter_library::msg::LibraryConfig,
    > {
        owner: processor_address.clone(),
        processor: processor_address.clone(),
        config: splitter_config,
    };
    let phase_shift_splitter_library_address = neutron_client
        .instantiate(
            code_id_splitter_library,
            "phase_shift_splitter".to_string(),
            instantiate_splitter_msg,
            None,
        )
        .await?;
    println!("Phase shift splitter library instantiated: {phase_shift_splitter_library_address}");

    // 4. Instantiate the phase shift forwarder library that will forward wBTC to the deposit account
    // of the maxBTC-WBTC supervault
    let phase_shift_forwarder_config = valence_forwarder_library::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[9].clone()), // Settlement account
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[8].clone()), // Deposit account of the maxBTC supervault
        forwarding_configs: vec![valence_forwarder_library::msg::UncheckedForwardingConfig {
            denom: UncheckedDenom::Native(params.program.deposit_token_on_neutron_denom.clone()),
            max_amount: Uint128::MAX,
        }],
        forwarding_constraints: ForwardingConstraints::default(),
    };
    let instantiate_phase_shift_forwarder_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_forwarder_library::msg::LibraryConfig,
    > {
        owner: processor_address.clone(),
        processor: processor_address.clone(),
        config: phase_shift_forwarder_config,
    };
    let phase_shift_forwarder_library_address = neutron_client
        .instantiate(
            code_id_forwarder_library,
            "phase_shift_forwarder".to_string(),
            instantiate_phase_shift_forwarder_msg,
            None,
        )
        .await?;
    println!("Phase shift forwarder library instantiated: {phase_shift_forwarder_library_address}");

    //////////                           //////////
    //////////  End of phase shift setup //////////
    //////////                           //////////

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

    // Now the rest
    let ica_deposit_account = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![deposit_splitter_library_address.clone()],
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
    println!("ICA deposit account instantiated: {ica_deposit_account_address}");

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
    println!("Mars deposit account instantiated: {mars_deposit_account_address}");

    let fbtc_deposit_account_instantiate_msg = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![fbtc_supervaults_lper_library_address.clone()],
    };
    let fbtc_supervault_deposit_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "fbtc_supervault_deposit".to_string(),
            fbtc_deposit_account_instantiate_msg,
            Some(params.general.owner.clone()),
            salts[2].clone(),
        )
        .await?;
    println!(
        "FBTC-WBTC supervault deposit account instantiated: {fbtc_supervault_deposit_account_address}"
    );

    let lbtc_deposit_account_instantiate_msg = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![lbtc_supervaults_lper_library_address.clone()],
    };
    let lbtc_supervault_deposit_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "lbtc_supervault_deposit".to_string(),
            lbtc_deposit_account_instantiate_msg,
            Some(params.general.owner.clone()),
            salts[3].clone(),
        )
        .await?;
    println!(
        "LBTC-WBTC supervault deposit account instantiated: {lbtc_supervault_deposit_account_address}"
    );

    let solvbtc_deposit_account_instantiate_msg = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![solvbtc_supervaults_lper_library_address.clone()],
    };

    let solvbtc_supervault_deposit_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "solvbtc_supervault_deposit".to_string(),
            solvbtc_deposit_account_instantiate_msg,
            Some(params.general.owner.clone()),
            salts[4].clone(),
        )
        .await?;
    println!(
        "SolvBTC-WBTC supervault deposit account instantiated: {solvbtc_supervault_deposit_account_address}"
    );

    let ebtc_deposit_account_instantiate_msg = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![ebtc_supervaults_lper_library_address.clone()],
    };
    let ebtc_supervault_deposit_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "ebtc_supervault_deposit".to_string(),
            ebtc_deposit_account_instantiate_msg,
            Some(params.general.owner.clone()),
            salts[5].clone(),
        )
        .await?;
    println!(
        "eBTC-WBTC supervault deposit account instantiated: {ebtc_supervault_deposit_account_address}"
    );

    let pumpbtc_deposit_account_instantiate_msg = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![pumpbtc_supervaults_lper_library_address.clone()],
    };
    let pumpbtc_supervault_deposit_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "pumpbtc_supervault_deposit".to_string(),
            pumpbtc_deposit_account_instantiate_msg,
            Some(params.general.owner.clone()),
            salts[6].clone(),
        )
        .await?;
    println!(
        "PumpBTC-WBTC supervault deposit account instantiated: {pumpbtc_supervault_deposit_account_address}"
    );

    let bedrockbtc_deposit_account_instantiate_msg = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![bedrockbtc_supervaults_lper_library_address.clone()],
    };
    let bedrockbtc_supervault_deposit_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "bedrockbtc_supervault_deposit".to_string(),
            bedrockbtc_deposit_account_instantiate_msg,
            Some(params.general.owner.clone()),
            salts[7].clone(),
        )
        .await?;
    println!(
        "bedrockbtc-WBTC supervault deposit account instantiated: {bedrockbtc_supervault_deposit_account_address}"
    );

    let maxbtc_btc_deposit_account_instantiate_msg = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![maxbtc_btc_supervaults_lper_library_address.clone()],
    };
    let maxbtc_btc_supervault_deposit_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "maxbtc_btc_supervault_deposit".to_string(),
            maxbtc_btc_deposit_account_instantiate_msg,
            Some(params.general.owner.clone()),
            salts[8].clone(),
        )
        .await?;
    println!(
        "MaxBTC-BTC supervault deposit account instantiated: {maxbtc_btc_supervault_deposit_account_address}"
    );

    let settlement_account = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![
            // This will contain all libraries that will execute actions on the settlement account
            clearing_queue_library_address.clone(),
            phase_shift_splitter_library_address.clone(),
            phase_shift_forwarder_library_address.clone(),
            fbtc_supervault_withdrawer_library_address.clone(),
            lbtc_supervault_withdrawer_library_address.clone(),
            solvbtc_supervault_withdrawer_library_address.clone(),
            ebtc_supervault_withdrawer_library_address.clone(),
            pumpbtc_supervault_withdrawer_library_address.clone(),
            bedrockbtc_supervault_withdrawer_library_address.clone(),
            maxbtc_issuer_library_address.clone(),
        ],
    };
    let settlement_account_address = neutron_client
        .instantiate2(
            code_id_base_account,
            "settlement".to_string(),
            settlement_account,
            Some(params.general.owner.clone()),
            salts[9].clone(),
        )
        .await?;
    println!("Settlement account instantiated: {settlement_account_address}");

    let denoms = NeutronDenoms {
        deposit_token: params.program.deposit_token_on_neutron_denom,
        ntrn: "untrn".to_string(),
        fbtc_supervault_lp: params.program.fbtc_supervault_lp_denom.clone(),
        lbtc_supervault_lp: params.program.lbtc_supervault_lp_denom.clone(),
        solvbtc_supervault_lp: params.program.solvbtc_supervault_lp_denom.clone(),
        ebtc_supervault_lp: params.program.ebtc_supervault_lp_denom.clone(),
        pumpbtc_supervault_lp: params.program.pumpbtc_supervault_lp_denom.clone(),
        bedrockbtc_supervault_lp: params.program.bedrockbtc_supervault_lp_denom.clone(),
        maxbtc_supervault_lp: "TBD".to_string(), // This will need to be updated after phase shift
    };

    let accounts = NeutronAccounts {
        ica: valence_ica_address.clone(),
        deposit: ica_deposit_account_address,
        mars_deposit: mars_deposit_account_address,
        settlement: settlement_account_address,
        fbtc_supervault_deposit: fbtc_supervault_deposit_account_address,
        lbtc_supervault_deposit: lbtc_supervault_deposit_account_address,
        solvbtc_supervault_deposit: solvbtc_supervault_deposit_account_address,
        ebtc_supervault_deposit: ebtc_supervault_deposit_account_address,
        pumpbtc_supervault_deposit: pumpbtc_supervault_deposit_account_address,
        bedrockbtc_supervault_deposit: bedrockbtc_supervault_deposit_account_address,
        maxbtc_supervault_deposit: maxbtc_btc_supervault_deposit_account_address,
    };

    let libraries = NeutronLibraries {
        deposit_splitter: deposit_splitter_library_address,
        dynamic_ratio_query_provider: dynamic_ratio_query_provider_address,
        mars_lending: mars_lending_library_address,
        clearing_queue: clearing_queue_library_address,
        ica_transfer: ica_ibc_transfer_library_address,
        fbtc_supervault_lper: fbtc_supervaults_lper_library_address,
        lbtc_supervault_lper: lbtc_supervaults_lper_library_address,
        solvbtc_supervault_lper: solvbtc_supervaults_lper_library_address,
        ebtc_supervault_lper: ebtc_supervaults_lper_library_address,
        pumpbtc_supervault_lper: pumpbtc_supervaults_lper_library_address,
        bedrockbtc_supervault_lper: bedrockbtc_supervaults_lper_library_address,
        maxbtc_supervault_lper: maxbtc_btc_supervaults_lper_library_address,
        phase_shift_splitter: phase_shift_splitter_library_address,
        phase_shift_forwarder: phase_shift_forwarder_library_address,
        phase_shift_maxbtc_issuer: maxbtc_issuer_library_address,
        phase_shift_fbtc_supervault_withdrawer: fbtc_supervault_withdrawer_library_address,
        phase_shift_lbtc_supervault_withdrawer: lbtc_supervault_withdrawer_library_address,
        phase_shift_solvbtc_supervault_withdrawer: solvbtc_supervault_withdrawer_library_address,
        phase_shift_ebtc_supervault_withdrawer: ebtc_supervault_withdrawer_library_address,
        phase_shift_pumpbtc_supervault_withdrawer: pumpbtc_supervault_withdrawer_library_address,
        phase_shift_bedrockbtc_supervault_withdrawer:
            bedrockbtc_supervault_withdrawer_library_address,
    };

    let coprocessor_app_ids = NeutronCoprocessorAppIds {
        clearing_queue: params.coprocessor_app.clearing_queue_coprocessor_app_id,
    };

    let neutron_cfg = NeutronStrategyConfig {
        grpc_url: params.general.grpc_url.clone(),
        grpc_port: params.general.grpc_port.clone(),
        chain_id: params.general.chain_id.clone(),
        mars_credit_manager: params.program.mars_credit_manager.clone(),
        fbtc_supervault: params.program.fbtc_supervault.clone(),
        lbtc_supervault: params.program.lbtc_supervault.clone(),
        solvbtc_supervault: params.program.solvbtc_supervault.clone(),
        ebtc_supervault: params.program.ebtc_supervault.clone(),
        pumpbtc_supervault: params.program.pumpbtc_supervault.clone(),
        bedrockbtc_supervault: params.program.bedrockbtc_supervault.clone(),
        maxbtc_supervault: "TBD".to_string(), // This will need to be updated after phase shift
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
        current_dir.join("deploy/src/neutron_strategy_config.toml"),
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

    let gaia_cfg = GaiaStrategyConfig {
        grpc_url: "grpc_url".to_string(),
        grpc_port: "grpc_port".to_string(),
        chain_id: "chain_id".to_string(),
        chain_denom: "uatom".to_string(),
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
