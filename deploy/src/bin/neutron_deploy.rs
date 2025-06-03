use std::{collections::BTreeMap, env, error::Error, fs, time::SystemTime};

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
    mars_lending: u64,
    clearing_queue: u64,
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

    // Predict all base accounts, we are going to store all salts for them as well. In total we need 6
    let mut salts = vec![];
    let mut predicted_base_accounts = vec![];
    for i in 0..6 {
        salts.push(format!("{}-{}", salt, i));
        let predicted_base_account_address = neutron_client
            .predict_instantiate2_addr(
                params.code_ids.base_account,
                format!("{}-{}", salt, i),
                my_address.clone(),
            )
            .await?
            .address;
        predicted_base_accounts.push(predicted_base_account_address);
    }

    // Predict the valence ICA address
    let predicted_valence_ica_address = neutron_client
        .predict_instantiate2_addr(
            params.code_ids.interchain_account,
            salt.clone(),
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
            params.code_ids.ica_ibc_transfer_library,
            "ica_ibc_transfer".to_string(),
            instantiate_ica_ibc_transfer_msg,
        )
        .await?;
    println!(
        "ICA IBC Transfer library instantiated: {}",
        ica_ibc_transfer_library_address
    );

    // Instantiate the deposit forwarder library
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
            params.code_ids.forwarder,
            "deposit_forwarder".to_string(),
            instantiate_deposit_forwarder_msg,
        )
        .await?;
    println!(
        "Deposit forwarder library instantiated: {}",
        deposit_forwarder_library_address
    );

    // Instantiate the Mars lending library
    let mars_lending_config = valence_mars_lending::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[1].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[2].clone()),
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
            params.code_ids.mars_lending,
            "mars_lending".to_string(),
            instantiate_mars_lending_msg,
        )
        .await?;
    println!(
        "Mars lending library instantiated: {}",
        mars_lending_library_address
    );

    // Instantiate phase forwarder library
    // Initially this one will forward funds to settlement account
    let phase_forwarder_config = valence_forwarder_library::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[2].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[5].clone()),
        forwarding_configs: vec![UncheckedForwardingConfig {
            denom: UncheckedDenom::Native(params.program.deposit_token_on_neutron_denom.clone()),
            max_amount: Uint128::MAX,
        }],
        forwarding_constraints: ForwardingConstraints::default(),
    };
    let instantiate_phase_forwarder_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_forwarder_library::msg::LibraryConfig,
    > {
        owner: params.general.owner.clone(),
        processor: processor_address.clone(),
        config: phase_forwarder_config,
    };
    let phase_forwarder_library_address = neutron_client
        .instantiate(
            params.code_ids.forwarder,
            "phase_forwarder".to_string(),
            instantiate_phase_forwarder_msg,
        )
        .await?;

    // Instantiate supervaults lper library
    let supervaults_lper_config = valence_supervaults_lper::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[3].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[4].clone()),
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
            params.code_ids.supervaults_lper,
            "supervaults_lper".to_string(),
            instantiate_supervaults_lper_msg,
        )
        .await?;
    println!(
        "Supervaults lper library instantiated: {}",
        supervaults_lper_library_address
    );

    // Instantiate supervaults withdrawer library
    let supervaults_withdrawer_config = valence_supervaults_withdrawer::msg::LibraryConfig {
        input_addr: LibraryAccountType::Addr(predicted_base_accounts[4].clone()),
        output_addr: LibraryAccountType::Addr(predicted_base_accounts[5].clone()),
        vault_addr: params.program.supervault.clone(),
        lw_config: valence_supervaults_withdrawer::msg::LiquidityWithdrawerConfig {
            asset_data: valence_library_utils::liquidity_utils::AssetData {
                asset1: params.program.supervault_asset1.clone(),
                asset2: params.program.supervault_asset2.clone(),
            },
            lp_denom: params.program.supervault_lp_denom.clone(),
        },
    };

    let instantiate_supervaults_withdrawer_msg = valence_library_utils::msg::InstantiateMsg::<
        valence_supervaults_withdrawer::msg::LibraryConfig,
    > {
        owner: params.general.owner.clone(),
        processor: processor_address.clone(),
        config: supervaults_withdrawer_config,
    };
    let supervaults_withdrawer_library_address = neutron_client
        .instantiate(
            params.code_ids.supervaults_withdrawer,
            "supervaults_withdrawer".to_string(),
            instantiate_supervaults_withdrawer_msg,
        )
        .await?;
    println!(
        "Supervaults withdrawer library instantiated: {}",
        supervaults_withdrawer_library_address
    );

    // Finally instantiate the clearing queue library
    let clearing_config = valence_clearing_queue::msg::LibraryConfig {
        settlement_acc_addr: LibraryAccountType::Addr(predicted_base_accounts[5].clone()),
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
            params.code_ids.clearing_queue,
            "clearing_queue".to_string(),
            instantiate_clearing_queue_msg,
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
            params.code_ids.interchain_account,
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
            params.code_ids.base_account,
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
            params.code_ids.base_account,
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

    let mars_withdraw_account = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![phase_forwarder_library_address.clone()],
    };
    let mars_withdraw_account_address = neutron_client
        .instantiate2(
            params.code_ids.base_account,
            "mars_withdraw".to_string(),
            mars_withdraw_account,
            Some(params.general.owner.clone()),
            salts[2].clone(),
        )
        .await?;
    println!(
        "Mars withdraw account instantiated: {}",
        mars_withdraw_account_address
    );

    let supervault_deposit_account = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![supervaults_lper_library_address.clone()],
    };
    let supervault_deposit_account_address = neutron_client
        .instantiate2(
            params.code_ids.base_account,
            "supervault_deposit".to_string(),
            supervault_deposit_account,
            Some(params.general.owner.clone()),
            salts[3].clone(),
        )
        .await?;
    println!(
        "Supervault deposit account instantiated: {}",
        supervault_deposit_account_address
    );

    let supervault_position_account = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![supervaults_withdrawer_library_address.clone()],
    };
    let supervault_position_account_address = neutron_client
        .instantiate2(
            params.code_ids.base_account,
            "supervault_position".to_string(),
            supervault_position_account,
            Some(params.general.owner.clone()),
            salts[4].clone(),
        )
        .await?;
    println!(
        "Supervault position account instantiated: {}",
        supervault_position_account_address
    );

    let settlement_account = valence_account_utils::msg::InstantiateMsg {
        admin: params.general.owner.clone(),
        approved_libraries: vec![clearing_queue_library_address.clone()],
    };
    let settlement_account_address = neutron_client
        .instantiate2(
            params.code_ids.base_account,
            "settlement".to_string(),
            settlement_account,
            Some(params.general.owner.clone()),
            salts[5].clone(),
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
        mars_withdraw: mars_withdraw_account_address,
        supervault_deposit: supervault_deposit_account_address,
        settlement: settlement_account_address,
    };

    let libraries = NeutronLibraries {
        deposit_forwarder: deposit_forwarder_library_address,
        mars_lending: mars_lending_library_address,
        phase_forwarder: phase_forwarder_library_address,
        supervault_lper: supervaults_lper_library_address,
        supervault_withdrawer: supervaults_withdrawer_library_address,
        clearing_queue: clearing_queue_library_address,
    };

    let neutron_cfg = NeutronStrategyConfig {
        grpc_url: "<Set GRPC here>".to_string(),
        grpc_port: "<Set GRPC port here>".to_string(),
        chain_id: "neutron-1".to_string(),
        mnemonic: "<taken from env>".to_string(),
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
