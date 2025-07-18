use std::{env, fs};

use cosmwasm_std::Binary;
use packages::types::inputs::ClearingQueueCoprocessorApp;
use serde::Deserialize;
use sp1_sdk::{HashableKey, SP1VerifyingKey};
use valence_authorization_utils::{
    authorization::{AtomicSubroutine, AuthorizationModeInfo, PermissionTypeInfo, Subroutine},
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
use wbtc_deploy::{INPUTS_DIR, OUTPUTS_DIR};
use wbtc_types::{
    labels::{
        ICA_TRANSFER_LABEL, LEND_AND_PROVIDE_LIQUIDITY_PHASE1_LABEL,
        LEND_AND_PROVIDE_LIQUIDITY_PHASE2_LABEL, MARS_WITHDRAW_LABEL, PHASE_SHIFT_STEP1_LABEL,
        PHASE_SHIFT_STEP2_LABEL, PHASE_SHIFT_STEP3_LABEL, PHASE_SHIFT_STEP4_LABEL,
        REGISTER_OBLIGATION_LABEL, SETTLE_OBLIGATION_LABEL,
    },
    neutron_config::NeutronStrategyConfig,
};

#[derive(Deserialize, Debug)]
struct Parameters {
    general: General,
    coprocessor_app: ClearingQueueCoprocessorApp,
}

#[derive(Deserialize, Debug)]
struct General {
    owner: String,
    strategist: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let mnemonic = env::var("MNEMONIC").expect("mnemonic must be provided");

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
            ntrn_strategy_config.libraries.ica_transfer.clone(),
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

    // Subroutine for Mars lending and Supervault LP providing which involves Split, Lend and Deposit to all supervaults
    // This one is for Phase 1
    let split_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "process_function".to_string(),
                params_restrictions: Some(vec![ParamRestriction::MustBeIncluded(vec![
                    "process_function".to_string(),
                    "split".to_string(),
                ])]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.deposit_splitter.clone(),
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

    let mut lend_and_provide_liquidity_phase1_functions = vec![];
    lend_and_provide_liquidity_phase1_functions.push(split_function.clone());
    lend_and_provide_liquidity_phase1_functions.push(lend_function.clone());
    // Create all provide liquidity functions for each supervault
    for address in [
        ntrn_strategy_config.libraries.fbtc_supervault_lper.clone(),
        ntrn_strategy_config.libraries.lbtc_supervault_lper.clone(),
        ntrn_strategy_config
            .libraries
            .solvbtc_supervault_lper
            .clone(),
        ntrn_strategy_config.libraries.ebtc_supervault_lper.clone(),
        ntrn_strategy_config
            .libraries
            .pumpbtc_supervault_lper
            .clone(),
        ntrn_strategy_config
            .libraries
            .bedrockbtc_supervault_lper
            .clone(),
    ] {
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
            contract_address: LibraryAccountType::Addr(address.clone()),
        };
        lend_and_provide_liquidity_phase1_functions.push(provide_liquidity_function);
    }

    let subroutine_lending_and_providing_liquidity_phase1 = Subroutine::Atomic(AtomicSubroutine {
        functions: lend_and_provide_liquidity_phase1_functions,
        retry_logic: None,
        expiration_time: None,
    });

    let authorization_lending_and_providing_liquidity = AuthorizationBuilder::new()
        .with_label(LEND_AND_PROVIDE_LIQUIDITY_PHASE1_LABEL)
        .with_mode(authorization_permissioned_mode.clone())
        .with_subroutine(subroutine_lending_and_providing_liquidity_phase1)
        .build();
    authorizations.push(authorization_lending_and_providing_liquidity);

    // Subroutine for Mars lending and Supervault LP providing which involves Split, Lend and Deposit to all supervaults
    // This one is for Phase 2
    let mut lend_and_provide_liquidity_phase2_functions = vec![];
    lend_and_provide_liquidity_phase2_functions.push(split_function.clone());
    lend_and_provide_liquidity_phase2_functions.push(lend_function.clone());
    // Create all provide liquidity functions for each supervault
    // This one includes the new maxBTC supervault
    for address in [
        ntrn_strategy_config.libraries.fbtc_supervault_lper.clone(),
        ntrn_strategy_config.libraries.lbtc_supervault_lper.clone(),
        ntrn_strategy_config
            .libraries
            .solvbtc_supervault_lper
            .clone(),
        ntrn_strategy_config.libraries.ebtc_supervault_lper.clone(),
        ntrn_strategy_config
            .libraries
            .pumpbtc_supervault_lper
            .clone(),
        ntrn_strategy_config
            .libraries
            .bedrockbtc_supervault_lper
            .clone(),
        ntrn_strategy_config
            .libraries
            .maxbtc_supervault_lper
            .clone(),
    ] {
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
            contract_address: LibraryAccountType::Addr(address.clone()),
        };
        lend_and_provide_liquidity_phase2_functions.push(provide_liquidity_function);
    }

    let subroutine_lending_and_providing_liquidity_phase2 = Subroutine::Atomic(AtomicSubroutine {
        functions: lend_and_provide_liquidity_phase2_functions,
        retry_logic: None,
        expiration_time: None,
    });

    let authorization_lending_and_providing_liquidity = AuthorizationBuilder::new()
        .with_label(LEND_AND_PROVIDE_LIQUIDITY_PHASE2_LABEL)
        .with_mode(authorization_permissioned_mode.clone())
        .with_subroutine(subroutine_lending_and_providing_liquidity_phase2)
        .build();
    authorizations.push(authorization_lending_and_providing_liquidity);

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

    //////// PHASE SHIFT AUTHORIZATIONS ////////
    // These authorizations are special, they will be executed from the Program owner via DAODAO in order
    // and involves multiple steps including updating configs with values we don't know during deployment
    let owner_mode =
        AuthorizationModeInfo::Permissioned(PermissionTypeInfo::WithoutCallLimit(vec![
            ntrn_params.general.owner.clone(),
        ]));

    // AUTHORIZATION 1 - STEPS:
    // 1) Update the maxBTC issuer with the right config (address of the maxBTC contract that we didn't have before)
    // 2) Withdraw from all supervaults the lent WBTC, we will get both WBTC and the other pair back
    // 3) Issue the maxBTC with all the BTC we got from the supervaults
    // 4) Trigger the split to return all LSTs to their corresponding deposit accounts

    let mut functions = vec![];
    // Update the maxBTC issuer with the right config
    let update_maxbtc_issuer_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "update_config".to_string(),
                params_restrictions: Some(vec![
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "input_addr".to_string(),
                    ]),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "output_addr".to_string(),
                    ]),
                ]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config
                .libraries
                .phase_shift_maxbtc_issuer
                .clone(),
        ),
    };
    functions.push(update_maxbtc_issuer_function);

    for supervault_withdrawer in [
        ntrn_strategy_config
            .libraries
            .phase_shift_fbtc_supervault_withdrawer
            .clone(),
        ntrn_strategy_config
            .libraries
            .phase_shift_lbtc_supervault_withdrawer
            .clone(),
        ntrn_strategy_config
            .libraries
            .phase_shift_solvbtc_supervault_withdrawer
            .clone(),
        ntrn_strategy_config
            .libraries
            .phase_shift_ebtc_supervault_withdrawer
            .clone(),
        ntrn_strategy_config
            .libraries
            .phase_shift_pumpbtc_supervault_withdrawer
            .clone(),
        ntrn_strategy_config
            .libraries
            .phase_shift_bedrockbtc_supervault_withdrawer
            .clone(),
    ] {
        let withdraw_function = AtomicFunction {
            domain: Domain::Main,
            message_details: MessageDetails {
                message_type: MessageType::CosmwasmExecuteMsg,
                message: Message {
                    name: "process_function".to_string(),
                    params_restrictions: Some(vec![ParamRestriction::MustBeIncluded(vec![
                        "process_function".to_string(),
                        "withdraw_liquidity".to_string(),
                    ])]),
                },
            },
            contract_address: LibraryAccountType::Addr(supervault_withdrawer.clone()),
        };
        functions.push(withdraw_function);
    }

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
            ntrn_strategy_config
                .libraries
                .phase_shift_maxbtc_issuer
                .clone(),
        ),
    };
    functions.push(issue_function);
    let split_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "process_function".to_string(),
                params_restrictions: Some(vec![ParamRestriction::MustBeIncluded(vec![
                    "process_function".to_string(),
                    "split".to_string(),
                ])]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.deposit_splitter.clone(),
        ),
    };
    functions.push(split_function);

    let subroutine_phase_shift_step1 = Subroutine::Atomic(AtomicSubroutine {
        functions,
        retry_logic: None,
        expiration_time: None,
    });
    let authorization_phase_shift_step1 = AuthorizationBuilder::new()
        .with_label(PHASE_SHIFT_STEP1_LABEL)
        .with_mode(owner_mode.clone())
        .with_subroutine(subroutine_phase_shift_step1)
        .build();
    authorizations.push(authorization_phase_shift_step1);

    // AUTHORIZATION 2 - STEPS:
    // 1) Update all the supervault lpers to use the new maxBTC supervault pair
    // 2) Trigger the deposit in all of them
    let mut functions = vec![];
    for supervault_lper in [
        ntrn_strategy_config.libraries.fbtc_supervault_lper.clone(),
        ntrn_strategy_config.libraries.lbtc_supervault_lper.clone(),
        ntrn_strategy_config
            .libraries
            .solvbtc_supervault_lper
            .clone(),
        ntrn_strategy_config.libraries.ebtc_supervault_lper.clone(),
        ntrn_strategy_config
            .libraries
            .pumpbtc_supervault_lper
            .clone(),
        ntrn_strategy_config
            .libraries
            .bedrockbtc_supervault_lper
            .clone(),
        ntrn_strategy_config
            .libraries
            .maxbtc_supervault_lper
            .clone(),
    ] {
        let update_function = AtomicFunction {
            domain: Domain::Main,
            message_details: MessageDetails {
                message_type: MessageType::CosmwasmExecuteMsg,
                message: Message {
                    name: "update_config".to_string(),
                    params_restrictions: Some(vec![
                        ParamRestriction::CannotBeIncluded(vec![
                            "update_config".to_string(),
                            "new_config".to_string(),
                            "input_addr".to_string(),
                        ]),
                        ParamRestriction::CannotBeIncluded(vec![
                            "update_config".to_string(),
                            "new_config".to_string(),
                            "output_addr".to_string(),
                        ]),
                    ]),
                },
            },
            contract_address: LibraryAccountType::Addr(supervault_lper.clone()),
        };
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
            contract_address: LibraryAccountType::Addr(supervault_lper.clone()),
        };
        functions.push(update_function);
        functions.push(provide_liquidity_function);
    }

    let subroutine_phase_shift_step2 = Subroutine::Atomic(AtomicSubroutine {
        functions,
        retry_logic: None,
        expiration_time: None,
    });
    let authorization_phase_shift_step2 = AuthorizationBuilder::new()
        .with_label(PHASE_SHIFT_STEP2_LABEL)
        .with_mode(owner_mode.clone())
        .with_subroutine(subroutine_phase_shift_step2)
        .build();
    authorizations.push(authorization_phase_shift_step2);

    // AUTHORIZATION 3 - STEPS:
    // 1) Withdraw half of the lent WBTC from Mars,
    // 2) Update the forwarder to send half of this amount to the maxBTC wBTC deposit account,
    // 3) Forward the funds
    // 4) Issue the maxBTC by depositing the other half of the WBTC
    // 5) Deposit in the maxBTC supervault

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
    let update_forwarder_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "update_config".to_string(),
                params_restrictions: Some(vec![
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "input_addr".to_string(),
                    ]),
                    ParamRestriction::CannotBeIncluded(vec![
                        "update_config".to_string(),
                        "new_config".to_string(),
                        "output_addr".to_string(),
                    ]),
                ]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.phase_shift_forwarder.clone(),
        ),
    };
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
            ntrn_strategy_config.libraries.phase_shift_forwarder.clone(),
        ),
    };
    let issue_maxbtc_function = AtomicFunction {
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
            ntrn_strategy_config
                .libraries
                .phase_shift_maxbtc_issuer
                .clone(),
        ),
    };
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
            ntrn_strategy_config
                .libraries
                .maxbtc_supervault_lper
                .clone(),
        ),
    };

    let subroutine_settle_obligation = AtomicSubroutineBuilder::new()
        .with_function(withdraw_function)
        .with_function(update_forwarder_function)
        .with_function(forward_function)
        .with_function(issue_maxbtc_function)
        .with_function(provide_liquidity_function)
        .build();
    let authorization_phase_shift_step3 = AuthorizationBuilder::new()
        .with_label(PHASE_SHIFT_STEP3_LABEL)
        .with_mode(owner_mode.clone())
        .with_subroutine(subroutine_settle_obligation)
        .build();
    authorizations.push(authorization_phase_shift_step3);

    // AUTHORIZATION 4 - STEPS:
    // 1) Update the new split config to use the new phase 2 ratios
    // 2) Update the clearing queue config to use the new settlement ratios
    // 3) Update the dynamic ratio query provide to use the new split ratios
    let update_splitter_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "update_config".to_string(),
                params_restrictions: Some(vec![ParamRestriction::CannotBeIncluded(vec![
                    "update_config".to_string(),
                    "new_config".to_string(),
                    "input_addr".to_string(),
                ])]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.deposit_splitter.clone(),
        ),
    };
    let update_clearing_queue_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "update_config".to_string(),
                params_restrictions: Some(vec![
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
                ]),
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config.libraries.clearing_queue.clone(),
        ),
    };
    let update_dynamic_ratio_query_provider_function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "update_ratios".to_string(),
                params_restrictions: None,
            },
        },
        contract_address: LibraryAccountType::Addr(
            ntrn_strategy_config
                .libraries
                .dynamic_ratio_query_provider
                .clone(),
        ),
    };

    let subroutine_phase_shift_step4 = AtomicSubroutineBuilder::new()
        .with_function(update_splitter_function)
        .with_function(update_clearing_queue_function)
        .with_function(update_dynamic_ratio_query_provider_function)
        .build();
    let authorization_phase_shift_step4 = AuthorizationBuilder::new()
        .with_label(PHASE_SHIFT_STEP4_LABEL)
        .with_mode(owner_mode)
        .with_subroutine(subroutine_phase_shift_step4)
        .build();
    authorizations.push(authorization_phase_shift_step4);

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
        validate_last_block_execution: false,
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
