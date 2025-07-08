#![no_std]

extern crate alloc;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use alloy_primitives::U256;
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

const SCALE_FACTOR: u64 = 1000000;
const CLEARING_QUEUE_LIBRARY_ADDRESS: &str =
    "neutron18dsl9523cvdj7dgn5ayev2kz09dhrha7fhsndv4p0urzetzvnyhsjsd3u4";

pub fn circuit(witnesses: Vec<Witness>) -> Vec<u8> {
    let withdraw_request_id = witnesses[0].as_data().unwrap();
    let withdraw_request_id = <[u8; 8]>::try_from(withdraw_request_id).unwrap();
    let withdraw_request_id = u64::from_le_bytes(withdraw_request_id);

    // Shares amount (U256 - 32 bytes)
    let withdraw_request_shares_amount = witnesses[1].as_data().unwrap();
    let withdraw_request_shares_amount_array =
        <[u8; 32]>::try_from(withdraw_request_shares_amount).unwrap();
    let withdraw_request_shares_amount = U256::from_le_bytes(withdraw_request_shares_amount_array);

    // Redemption rate (U256 - 32 bytes)
    let withdraw_request_redemption_rate = witnesses[2].as_data().unwrap();
    let withdraw_request_redemption_rate_array =
        <[u8; 32]>::try_from(withdraw_request_redemption_rate).unwrap();
    let withdraw_request_redemption_rate =
        U256::from_le_bytes(withdraw_request_redemption_rate_array);

    let recipient = witnesses[3].as_data().unwrap();
    let recipient = String::from_utf8(recipient.to_vec()).unwrap();

    // Calculate the amounts to be paid out by doing (shares × current_redemption_rate) / initial_redemption_rate
    let withdraw_request_amount = (withdraw_request_shares_amount
        * withdraw_request_redemption_rate)
        / U256::from(SCALE_FACTOR);

    let withdraw_request_amount_u128: u128 = withdraw_request_amount
        .try_into()
        .expect("U256 value too large to fit in u128");

    let clearing_queue_msg: ExecuteMsg<FunctionMsgs, LibraryConfigUpdate> =
        ExecuteMsg::ProcessFunction(FunctionMsgs::RegisterObligation {
            recipient,
            payout_amount: Uint128::from(withdraw_request_amount_u128),
            id: Uint64::from(withdraw_request_id),
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
    };

    serde_json::to_vec(&msg).unwrap()
}
