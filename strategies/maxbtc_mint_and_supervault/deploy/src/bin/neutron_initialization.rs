use std::{env, fs};

use cosmwasm_std::Binary;
use maxbtc_mint_and_supervault_deploy::{INPUTS_DIR, OUTPUTS_DIR};
use maxbtc_mint_and_supervault_types::neutron_config::NeutronStrategyConfig;
use packages::utils::crypto_provider::setup_crypto_provider;
use packages::verification::VERIFICATION_ROUTE;
use packages::{
    labels::{
        ICA_TRANSFER_LABEL, MAXBTC_ISSUE_LABEL, PROVIDE_LIQUIDIY_LABEL, REGISTER_OBLIGATION_LABEL,
        SETTLE_OBLIGATION_LABEL,
    },
    types::inputs::ClearingQueueCoprocessorApp,
};
use serde::Deserialize;
use sp1_sdk::{HashableKey, SP1VerifyingKey};
use valence_authorization_utils::{
    authorization::{AuthorizationModeInfo, PermissionTypeInfo},
    authorization_message::{Message, MessageDetails, MessageType, ParamRestriction},
    builders::{AtomicSubroutineBuilder, AuthorizationBuilder},
    domain::Domain,
    function::AtomicFunction,
    zk_authorization::ZkAuthorizationInfo,
};
use valence_domain_clients::{
    clients::{coprocessor::CoprocessorClient, neutron::NeutronClient},
    coprocessor::base_client::CoprocessorBaseClient,
    cosmos::wasm_client::WasmClient,
};
use valence_library_utils::LibraryAccountType;

#[derive(Deserialize, Debug)]
struct Parameters {
    general: General,
    coprocessor_app: ClearingQueueCoprocessorApp,
}

#[derive(Deserialize, Debug)]
struct General {
    strategist: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let mnemonic = env::var("MNEMONIC").expect("mnemonic must be provided");

    setup_crypto_provider().await?;

    let current_dir = env::current_dir()?;

    let ntrn_params = fs::read_to_string(current_dir.join(format!("{INPUTS_DIR}/neutron.toml")))
        .expect("Failed to read file");

    let ntrn_params: Parameters =
        toml::from_str(&ntrn_params).expect("Failed to parse Neutron parameters");

    let strategist = ntrn_params.general.strategist;

    let ntrn_cfg =
        fs::read_to_string(current_dir.join(format!("{OUTPUTS_DIR}/neutron_strategy_config.toml")))
            .expect("Failed to read file");

    let ntrn_strategy_config: NeutronStrategyConfig =
        toml::from_str(&ntrn_cfg).expect("Failed to parse Neutron strategy config");

    let authorization_contract = ntrn_strategy_config.authorizations;

    let neutron_client = NeutronClient::new(
        &ntrn_strategy_config.grpc_url,
        &ntrn_strategy_config.grpc_port,
        &mnemonic,
        &ntrn_strategy_config.chain_id,
    )
    .await?;

    let mut authorizations = vec![];

    // All authorizations except the phase shift one will be called by strategist
    let authorization_permissioned_mode =
        AuthorizationModeInfo::Permissioned(PermissionTypeInfo::WithoutCallLimit(vec![
            strategist.clone(),
        ]));

    let provide_liquidity_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "process_function".to_string(),
                params_restrictions: Some(vec![ParamRestriction::MustBeIncluded(vec![
                    "process_function".to_string(),
                    "provide_liquidity".to_string(),
                ])]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.supervault_lper.clone(),
        ),
    };

    let subroutine_providing_liquidity = AtomicSubroutineBuilder::new()
        .with_function(provide_liquidity_function)
        .build();

    let authorization_providing_liquidity = AuthorizationBuilder::new()
        .with_label(PROVIDE_LIQUIDIY_LABEL)
        .with_mode(authorization_permissioned_mode.clone())
        .with_subroutine(subroutine_providing_liquidity)
        .build();
    authorizations.push(authorization_providing_liquidity);

    // Subroutine for ICA Transfer
    // Involves updating the amount and trigger the transfer
    let update_amount_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                // Well only allow updating the amount, any other update will be rejected
                name: "update_config".to_string(),
                params_restrictions: Some(vec![
                    ParamRestriction::MustBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "amount".to_string(),
                    ]),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "input_addr".to_string(),
                    ]),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "denom".to_string(),
                    ]),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "receiver".to_string(),
                    ]),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "memo".to_string(),
                    ]),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "remote_chain_info".to_string(),
                    ]),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "denom_to_pfm_map".to_string(),
                    ]),
                ]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.ica_transfer_gaia.clone(),
        ),
    };

    let transfer_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "process_function".to_string(),
                // Only allow calling transfer
                params_restrictions: Some(vec![ParamRestriction::MustBeIncluded(vec![
                    "process_function".to_string(),
                    "transfer".to_string(),
                ])]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.ica_transfer_gaia,
        ),
    };

    let subroutine_ica_transfer = AtomicSubroutineBuilder::new()
        .with_function(update_amount_function)
        .with_function(transfer_function)
        .build();

    let authorization_ica_transfer = AuthorizationBuilder::new()
        .with_label(ICA_TRANSFER_LABEL)
        .with_mode(authorization_permissioned_mode.clone())
        .with_subroutine(subroutine_ica_transfer)
        .build();

    authorizations.push(authorization_ica_transfer);

    // Authorization for issuing maxBTC
    let issue_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "process_function".to_string(),
                params_restrictions: Some(vec![ParamRestriction::MustBeIncluded(vec![
                    "process_function".to_string(),
                    "issue".to_string(),
                ])]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.maxbtc_issuer.clone(),
        ),
    };

    let subroutine_issue = AtomicSubroutineBuilder::new()
        .with_function(issue_function)
        .build();
    let authorization_issue = AuthorizationBuilder::new()
        .with_label(MAXBTC_ISSUE_LABEL)
        .with_mode(authorization_permissioned_mode.clone())
        .with_subroutine(subroutine_issue)
        .build();
    authorizations.push(authorization_issue);

    // Authorization for settle obligation
    let settle_obligation_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "process_function".to_string(),
                params_restrictions: Some(vec![ParamRestriction::MustBeIncluded(vec![
                    "process_function".to_string(),
                    "settle_next_obligation".to_string(),
                ])]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.clearing_queue.clone(),
        ),
    };

    let subroutine_settle_obligation = AtomicSubroutineBuilder::new()
        .with_function(settle_obligation_function)
        .build();
    let authorization_settle_obligation = AuthorizationBuilder::new()
        .with_label(SETTLE_OBLIGATION_LABEL)
        .with_mode(authorization_permissioned_mode.clone())
        .with_subroutine(subroutine_settle_obligation)
        .build();
    authorizations.push(authorization_settle_obligation);

    // Add all authorizations to the authorization contract
    let create_authorizations = valence_authorization_utils::msg::ExecuteMsg::PermissionedAction(
        valence_authorization_utils::msg::PermissionedMsg::CreateAuthorizations { authorizations },
    );

    neutron_client
        .execute_wasm(&authorization_contract, create_authorizations, vec![], None)
        .await?;
    println!("Authorizations created successfully");

    // Get the VK for the coprocessor app
    let coprocessor_client = CoprocessorClient::default();
    let program_vk = coprocessor_client
        .get_vk(
            &ntrn_params
                .coprocessor_app
                .clearing_queue_coprocessor_app_id,
        )
        .await?;

    let sp1_program_vk: SP1VerifyingKey = bincode::deserialize(&program_vk)?;

    // Now we create the zk authorization
    let zk_authorization = ZkAuthorizationInfo {
        label: REGISTER_OBLIGATION_LABEL.to_string(),
        mode: authorization_permissioned_mode,
        registry: 0,
        vk: Binary::from(sp1_program_vk.bytes32().as_bytes()),
        verification_route: VERIFICATION_ROUTE.to_string(),
        validate_last_block_execution: false,
        metadata_hash: Binary::default(),
    };

    let create_zk_authorization = valence_authorization_utils::msg::ExecuteMsg::PermissionedAction(
        valence_authorization_utils::msg::PermissionedMsg::CreateZkAuthorizations {
            zk_authorizations: vec![zk_authorization],
        },
    );

    neutron_client
        .execute_wasm(
            &authorization_contract,
            create_zk_authorization,
            vec![],
            None,
        )
        .await?;
    println!("ZK Authorization created successfully");

    // TODO: Transfer ownership of authorization contract to owner

    Ok(())
}
