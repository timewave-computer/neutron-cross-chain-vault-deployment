#![no_std]

extern crate alloc;

use alloc::{string::ToString as _, vec::Vec};
use alloy_rpc_types_eth::EIP1186AccountProofResponse;
use clearing_queue_core::WithdrawRequest;
use cosmwasm_std::{Uint64, Uint128, to_json_binary};
use valence_authorization_utils::{
    authorization::{AtomicSubroutine, AuthorizationMsg, Priority, Subroutine},
    authorization_message::{Message, MessageDetails, MessageType},
    domain::Domain,
    function::AtomicFunction,
    msg::ProcessorMessage,
    zk_authorization::ZkMessage,
};
use valence_clearing_queue_supervaults::msg::{FunctionMsgs, LibraryConfigUpdate};
use valence_coprocessor::Witness;
use valence_library_utils::{LibraryAccountType, msg::ExecuteMsg};

const SCALE_FACTOR: u128 = 100000000;
const CLEARING_QUEUE_LIBRARY_ADDRESS: &str =
    "neutron1pdp6mty3ykchjxksj9hakupma3atyrah27mtws3ph2matjkhg7qse70m8g";

pub fn circuit(witnesses: Vec<Witness>) -> Vec<u8> {
    /*let state = witnesses[0].as_state_proof().unwrap();
    let root = state.root;
    let proof: EIP1186AccountProofResponse =
        bincode::serde::decode_from_slice(&state.proof, bincode::config::standard())
            .unwrap()
            .0;

    let withdraw = witnesses[1].as_data().unwrap();
    let withdraw: WithdrawRequest =
        bincode::serde::decode_from_slice(&withdraw, bincode::config::standard())
            .unwrap()
            .0;

    assert!(!withdraw.redemptionRate.is_zero());

    clearing_queue_core::verify_proof(&proof, &withdraw, &root).unwrap();

    // Calculate the amounts to be paid out by doing (shares × current_redemption_rate) / initial_redemption_rate
    let shares: u128 = withdraw.sharesAmount.try_into().unwrap();
    let rate: u128 = withdraw.redemptionRate.try_into().unwrap();
    let withdraw_request_amount = shares
        .checked_mul(rate)
        .unwrap()
        .checked_div(SCALE_FACTOR)
        .unwrap();

    let clearing_queue_msg: ExecuteMsg<FunctionMsgs, LibraryConfigUpdate> =
        ExecuteMsg::ProcessFunction(FunctionMsgs::RegisterObligation {
            recipient: withdraw.receiver,
            payout_amount: Uint128::from(withdraw_request_amount),
            id: Uint64::from(withdraw.id),
        });

    let processor_msg = ProcessorMessage::CosmwasmExecuteMsg {
        msg: to_json_binary(&clearing_queue_msg).unwrap(),
    };

    let function = AtomicFunction {
        domain: Domain::Main,
        message_details: MessageDetails {
            message_type: MessageType::CosmwasmExecuteMsg,
            message: Message {
                name: "process_function".to_string(),
                params_restrictions: None,
            },
        },
        contract_address: LibraryAccountType::Addr(CLEARING_QUEUE_LIBRARY_ADDRESS.to_string()),
    };

    let subroutine = AtomicSubroutine {
        functions: Vec::from([function]),
        retry_logic: None,
        expiration_time: None,
    };

    let message = AuthorizationMsg::EnqueueMsgs {
        id: 0,
        msgs: Vec::from([processor_msg]),
        subroutine: Subroutine::Atomic(subroutine),
        priority: Priority::Medium,
        expiration_time: None,
    };

    let msg = ZkMessage {
        registry: 0,
        block_number: 0,
        domain: Domain::Main,
        authorization_contract: None,
        message,
    };*/

    Vec::new()
}
