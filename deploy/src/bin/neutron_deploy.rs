use std::{collections::BTreeMap, env, error::Error, fs, time::SystemTime};

use cosmwasm_std::{Uint128, Uint64};
use serde::Deserialize;
use types::neutron_config::{
    IcaAccount, NeutronAccounts, NeutronDenoms, NeutronLibraries, NeutronStrategyConfig,
};
use valence_domain_clients::{
    clients::neutron::NeutronClient,
    cosmos::{grpc_client::GrpcSigningClient, wasm_client::WasmClient},
};
use valence_forwarder_library::msg::{ForwardingConstraints, UncheckedForwardingConfig};
use valence_library_utils::{denoms::UncheckedDenom, LibraryAccountType};

#[derive(Deserialize, Debug)]
struct Parameters {
    general: General,
    code_ids: CodeIds,
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
struct CodeIds {
    authorization: u64,
    processor: u64,
    base_account: u64,
    interchain_account: u64,
    forwarder: u64,
    ica_ibc_transfer_library: u64,
    supervaults_lper: u64,
    supervaults_withdrawer: u64,
    mars_position_manager: u64,
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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv::dotenv().ok();
    let mnemonic = env::var("MNEMONIC").expect("mnemonic must be provided");

    let current_dir = env::current_dir()?;
    let parameters = fs::read_to_string(current_dir.join("deploy/src/neutron.toml"))
        .expect("Failed to read file");

    let params: Parameters = toml::from_str(&parameters).expect("Failed to parse TOML");

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

    let code_id_authorization = params.code_ids.authorization;
    let code_id_processor = params.code_ids.processor;

    let now = SystemTime::now();
    let salt = now
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs()
        .to_string();

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

    // Predict address of the valence_interchain_account
    let predicted_valence_ica_address = neutron_client
        .predict_instantiate2_addr(
            params.code_ids.interchain_account,
            salt.clone(),
            my_address.clone(),
        )
        .await?
        .address;

    // Predict address of the ica deposit account
    let ica_deposit_salt = format!("ica_deposit_{}", salt);
    let predicted_ica_deposit_account = neutron_client
        .predict_instantiate2_addr(
            params.code_ids.base_account,
            ica_deposit_salt.clone(),
            my_address.clone(),
        )
        .await?
        .address;

    // Instantiate the ICA ibc transfer library
    let config = valence_ica_ibc_transfer::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_valence_ica_address.clone()),
        // The strategist needs to update this to the actual amount that needs to be transferred. There will be an authorization for this.
        amount: Uint128::one(),
        denom: params.ica.deposit_token_on_hub_denom.clone(),
        receiver: predicted_ica_deposit_account.clone(),
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
        .instantiate2(
            params.code_ids.ica_ibc_transfer_library,
            "ica_ibc_transfer".to_string(),
            instantiate_ica_ibc_transfer_msg,
            Some(params.general.owner.clone()),
            salt.clone(),
        )
        .await?;
    println!(
        "ICA IBC Transfer library instantiated: {}",
        ica_ibc_transfer_library_address
    );

    // Instantiate the Valence ICA now
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
            params.code_ids.interchain_account,
            "valence_ica".to_string(),
            valence_ica_instantiate_msg,
            Some(params.general.owner.clone()),
            salt.clone(),
        )
        .await?;
    println!("Valence ICA instantiated: {}", valence_ica_address);

    // Predict the Mars deposit account
    let mars_deposit_salt = format!("mars_deposit{}", salt);
    let predicted_mars_deposit_account = neutron_client
        .predict_instantiate2_addr(
            params.code_ids.base_account,
            mars_deposit_salt.clone(),
            my_address.clone(),
        )
        .await?
        .address;

    // Instantiate the deposit forwarder library
    let deposit_forwarder_config = valence_forwarder_library::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_valence_ica_address.clone()),
        output_addr: LibraryAccountType::Addr(predicted_mars_deposit_account.clone()),
        forwarding_configs: vec![UncheckedForwardingConfig {
            denom: UncheckedDenom::Native(params.program.deposit_token_on_neutron_denom.clone()),
            max_amount: Uint128::MAX,
        }],
        forwarding_constraints: ForwardingConstraints::default(),
    };

    // Instantiate the deposit forwarder library
    let instantiate_deposit_forwarder_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_forwarder_library::msg::LibraryConfig,
    > {
        owner: params.general.owner.clone(),
        processor: processor_address.clone(),
        config: deposit_forwarder_config,
    };

    let deposit_forwarder_library_address = neutron_client
        .instantiate2(
            params.code_ids.forwarder,
            "deposit_forwarder".to_string(),
            instantiate_deposit_forwarder_msg,
            Some(params.general.owner.clone()),
            salt.clone(),
        )
        .await?;
    println!(
        "Deposit forwarder library instantiated: {}",
        deposit_forwarder_library_address
    );

    // Instantiate the ica deposit account
    let deposit_account_instantiate_msg = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![deposit_forwarder_library_address.clone()],
    };

    let deposit_account_address = neutron_client
        .instantiate2(
            params.code_ids.base_account,
            "deposit_account".to_string(),
            deposit_account_instantiate_msg,
            Some(params.general.owner.clone()),
            ica_deposit_salt.clone(),
        )
        .await?;
    println!("Deposit account instantiated: {}", deposit_account_address);

    let denoms = NeutronDenoms {
        wbtc: "ibc/wbtc...".to_string(),
        ntrn: "untrn".to_string(),
        supervault_lp: "factory/neutron1.../supervault".to_string(),
    };

    let accounts = NeutronAccounts {
        deposit: "neutron1deposit...".to_string(),
        mars: "neutron1mars...".to_string(),
        supervault: "neutron1supervault...".to_string(),
        settlement: "neutron1settlement...".to_string(),
        gaia_ica: IcaAccount {
            library_account: "neutron1ica...".to_string(),
            remote_addr: "cosmos1ica...".to_string(),
        },
    };

    let libraries = NeutronLibraries {
        clearing: "neutron1clearing...".to_string(),
        mars_lending: "neutron1mars_lending...".to_string(),
        supervaults_depositor: "neutron1supervaults_depositor...".to_string(),
        deposit_forwarder: "neutron1deposit_fwd...".to_string(),
        ica_ibc_transfer: "neutron1ica_ibc_transfer...".to_string(),
    };

    let _neutron_cfg = NeutronStrategyConfig {
        grpc_url: "https://0.0.0.0".to_string(),
        grpc_port: "12345".to_string(),
        chain_id: "neutron-1".to_string(),
        mnemonic: "racoon racoon racoon racoon racoon racoon".to_string(),
        mars_pool: "neutron1mars...".to_string(),
        supervault: "neutron1supervault...".to_string(),
        denoms,
        accounts,
        libraries,
        min_ibc_fee: Uint128::one(),
        authorizations: "neutron1authorizations...".to_string(),
        processor: "neutron1processor...".to_string(),
    };

    Ok(())
}
