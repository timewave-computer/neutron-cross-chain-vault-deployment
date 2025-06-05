use std::{env, error::Error, fs};

use cosmwasm_std::{to_json_binary, Binary, Uint128};
use serde::Deserialize;
use types::{
    gaia_config::GaiaStrategyConfig,
    labels::{
        ICA_TRANSFER_LABEL, MARS_LEND_LABEL, MARS_WITHDRAW_LABEL, PHASE_CHANGE_LABEL,
        REGISTER_OBLIGATION_LABEL, SETTLE_OBLIGATION_LABEL, SUPERVAULT_LP_LABEL,
    },
    neutron_config::NeutronStrategyConfig,
};
use valence_authorization_utils::{
    authorization::{AuthorizationModeInfo, PermissionTypeInfo},
    authorization_message::{Message, MessageDetails, MessageType, ParamRestriction},
    builders::{AtomicSubroutineBuilder, AuthorizationBuilder},
    domain::Domain,
    function::AtomicFunction,
    zk_authorization::ZkAuthorizationInfo,
};
use valence_domain_clients::{clients::neutron::NeutronClient, cosmos::wasm_client::WasmClient};
use valence_library_utils::LibraryAccountType;

#[derive(Deserialize, Debug)]
struct Parameters {
    general: General,
    ica: Ica,
    program: Program,
}

#[derive(Deserialize, Debug)]
struct General {
    strategist: String,
}

#[derive(Deserialize, Debug)]
struct Ica {
    deposit_token_on_hub_denom: String,
}

#[derive(Deserialize, Debug)]
struct Program {
    clearing_queue_coprocessor_app_vk: Binary,
    domain_vk: Binary,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv::dotenv().ok();
    let mnemonic = env::var("MNEMONIC").expect("mnemonic must be provided");

    let current_dir = env::current_dir()?;

    let ntrn_params = fs::read_to_string(current_dir.join("deploy/src/neutron.toml"))
        .expect("Failed to read file");

    let ntrn_params: Parameters =
        toml::from_str(&ntrn_params).expect("Failed to parse Neutron parameters");

    let strategist = ntrn_params.general.strategist;

    let ntrn_cfg = fs::read_to_string(current_dir.join("deploy/src/neutron_strategy_config.toml"))
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

    // All authorizations except the phase shift will be called by strategist
    let authorization_permissioned_mode =
        AuthorizationModeInfo::Permissioned(PermissionTypeInfo::WithoutCallLimit(vec![
            strategist.clone()
        ]));

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
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "eureka_config".to_string(),
                    ]),
                ]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.ica_transfer.clone(),
        ),
    };

    let transfer_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "process_function".to_string(),
                // Only Transfer can be called because Eureka will fail as there is no config, no need for restrictions
                params_restrictions: None,
            },
        },
        contract_address: LibraryAccountType::Addr(ntrn_strategy_config.libraries.ica_transfer),
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

    // Subroutine for Mars lending which involves forward and lend
    let forward_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "process_function".to_string(),
                params_restrictions: Some(vec![ParamRestriction::MustBeIncluded(vec![
                    "process_function".to_string(),
                    "forward".to_string(),
                ])]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.deposit_forwarder.clone(),
        ),
    };
    let lend_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "process_function".to_string(),
                params_restrictions: Some(vec![ParamRestriction::MustBeIncluded(vec![
                    "process_function".to_string(),
                    "lend".to_string(),
                ])]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.mars_lending.clone(),
        ),
    };
    let subroutine_mars_lending = AtomicSubroutineBuilder::new()
        .with_function(forward_function.clone())
        .with_function(lend_function)
        .build();
    let authorization_mars_lending = AuthorizationBuilder::new()
        .with_label(MARS_LEND_LABEL)
        .with_mode(authorization_permissioned_mode.clone())
        .with_subroutine(subroutine_mars_lending)
        .build();
    authorizations.push(authorization_mars_lending);

    // Authorization for Supervault lp
    let supervault_lp_function = AtomicFunction {
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

    let subroutine_supervault_lp = AtomicSubroutineBuilder::new()
        .with_function(forward_function)
        .with_function(supervault_lp_function)
        .build();

    let authorization_supervault_lp = AuthorizationBuilder::new()
        .with_label(SUPERVAULT_LP_LABEL)
        .with_mode(authorization_permissioned_mode.clone())
        .with_subroutine(subroutine_supervault_lp)
        .build();
    authorizations.push(authorization_supervault_lp);

    // Authorization for Mars withdrawing
    let withdraw_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "process_function".to_string(),
                params_restrictions: Some(vec![ParamRestriction::MustBeIncluded(vec![
                    "process_function".to_string(),
                    "withdraw".to_string(),
                ])]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.mars_lending.clone(),
        ),
    };
    let subroutine_mars_withdraw = AtomicSubroutineBuilder::new()
        .with_function(withdraw_function)
        .build();
    let authorization_mars_withdraw = AuthorizationBuilder::new()
        .with_label(MARS_WITHDRAW_LABEL)
        .with_mode(authorization_permissioned_mode.clone())
        .with_subroutine(subroutine_mars_withdraw)
        .build();
    authorizations.push(authorization_mars_withdraw);

    // Authorization for settle obligation
    let settle_obligation_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "process_function".to_string(),
                params_restrictions: Some(vec![ParamRestriction::MustBeIncluded(vec![
                    "process_function".to_string(),
                    "settle_obligation".to_string(),
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

    // Phase change authorization
    // This will include 3 operations:
    // 1. Update the phase in the clearing queue, by updating the config and setting the boolean for supervaults phase to true
    // 2. Update the deposit forwarder to now forward to supervaults deposit account
    // 3. Update the Mars lending library to now have the deposit account as output account
    let clearing_queue_update_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "update_config".to_string(),
                params_restrictions: Some(vec![
                    ParamRestriction::MustBeValue(
                        vec![
                            "update_config".to_string(),
                            "new_config".to_string(),
                            "super_vaults_phase".to_string(),
                        ],
                        to_json_binary(&true).unwrap(),
                    ),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "settlement_acc_addr".to_string(),
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
                        "latest_id".to_string(),
                    ]),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "supervault_addr".to_string(),
                    ]),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "supervault_sender".to_string(),
                    ]),
                ]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.clearing_queue.clone(),
        ),
    };

    let deposit_forwarder_update_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "update_config".to_string(),
                params_restrictions: Some(vec![
                    ParamRestriction::MustBeValue(
                        vec![
                            "update_config".to_string(),
                            "new_config".to_string(),
                            "output_addr".to_string(),
                        ],
                        to_json_binary(&ntrn_strategy_config.accounts.supervault_deposit).unwrap(),
                    ),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "input_addr".to_string(),
                    ]),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "forwarding_configs".to_string(),
                    ]),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "forwarding_constraints".to_string(),
                    ]),
                ]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.deposit_forwarder.clone(),
        ),
    };

    let mars_lending_update_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "update_config".to_string(),
                params_restrictions: Some(vec![
                    ParamRestriction::MustBeValue(
                        vec![
                            "update_config".to_string(),
                            "new_config".to_string(),
                            "output_addr".to_string(),
                        ],
                        to_json_binary(&ntrn_strategy_config.accounts.deposit).unwrap(),
                    ),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "input_addr".to_string(),
                    ]),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "credit_manager_addr".to_string(),
                    ]),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "denom".to_string(),
                    ]),
                ]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.mars_lending.clone(),
        ),
    };

    let subroutine_phase_change = AtomicSubroutineBuilder::new()
        .with_function(clearing_queue_update_function)
        .with_function(deposit_forwarder_update_function)
        .with_function(mars_lending_update_function)
        .build();

    // This one will only be called once by the strategist and can't be undone
    let authorization_phase_change = AuthorizationBuilder::new()
        .with_label(PHASE_CHANGE_LABEL)
        .with_mode(AuthorizationModeInfo::Permissioned(
            PermissionTypeInfo::WithCallLimit(vec![(strategist.clone(), Uint128::one())]),
        ))
        .with_subroutine(subroutine_phase_change)
        .build();

    authorizations.push(authorization_phase_change);

    // Add all authorizations to the authorization contract
    let create_authorizations =
        valence_authorization_utils::msg::PermissionedMsg::CreateAuthorizations { authorizations };

    neutron_client
        .execute_wasm(&authorization_contract, create_authorizations, vec![], None)
        .await?;
    println!("Authorizations created successfully");

    // Now we create the zk authorization
    let zk_authorization = ZkAuthorizationInfo {
        label: REGISTER_OBLIGATION_LABEL.to_string(),
        mode: authorization_permissioned_mode,
        registry: 0,
        vk: ntrn_params.program.clearing_queue_coprocessor_app_vk,
        domain_vk: ntrn_params.program.domain_vk,
        validate_last_block_execution: false,
    };

    let create_zk_authorization =
        valence_authorization_utils::msg::PermissionedMsg::CreateZkAuthorizations {
            zk_authorizations: vec![zk_authorization],
        };
    neutron_client
        .execute_wasm(
            &authorization_contract,
            create_zk_authorization,
            vec![],
            None,
        )
        .await?;
    println!("ZK Authorization created successfully");

    // Last thing we will do is register the ICA on the valence ICA
    let register_ica_msg = valence_account_utils::ica::ExecuteMsg::RegisterIca {};
    neutron_client
        .execute_wasm(
            &ntrn_strategy_config.accounts.ica,
            register_ica_msg,
            vec![cosmrs::Coin::new(1_000_000u128, "untrn").unwrap()],
            None,
        )
        .await?;

    println!("Registering ICA");

    // Let's wait enough time for the transaction to succeed and the ICA to be registered
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;

    // Let's query now to get the ICA address
    let query_ica = valence_account_utils::ica::QueryMsg::IcaState {};
    let ica_state: valence_account_utils::ica::IcaState = neutron_client
        .query_contract_state(&ntrn_strategy_config.accounts.ica, query_ica)
        .await?;

    let ica_address = match ica_state {
        valence_account_utils::ica::IcaState::Created(ica_information) => ica_information.address,
        _ => {
            panic!("ICA creation failed!");
        }
    };

    let gaia_cfg = GaiaStrategyConfig {
        grpc_url: "grpc_url".to_string(),
        grpc_port: "grpc_port".to_string(),
        chain_id: "chain_id".to_string(),
        chain_denom: "uatom".to_string(),
        deposit_denom: ntrn_params.ica.deposit_token_on_hub_denom,
        ica_address,
    };

    // Write the Gaia strategy config to a file
    let gaia_cfg_path = current_dir.join("deploy/src/gaia_strategy_config.toml");
    fs::write(
        gaia_cfg_path,
        toml::to_string(&gaia_cfg).expect("Failed to serialize Gaia strategy config"),
    )
    .expect("Failed to write Gaia strategy config to file");

    Ok(())
}
