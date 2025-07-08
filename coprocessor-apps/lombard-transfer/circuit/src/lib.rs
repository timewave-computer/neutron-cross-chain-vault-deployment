#![no_std]

extern crate alloc;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use alloy_primitives::{Address, Bytes};
use alloy_sol_types::{SolCall, SolValue, sol};
use serde_json::Value;
use valence_coprocessor::Witness;

// Currently fee is 0.20$ which translates currently to 193. So we'll set a ceiling of 10 times that.
const MAX_FEE_ALLOWED: u64 = 1930;
// The library this will be executed on:
const EUREKA_TRANSFER_LIBRARY_CONTRACT: &str = "0xc9e967dbfe888e3999697cb289a1212443928450";

// Memo fields that need to be validated
const DEST_CALLBACK: &str = "lom13ehuhysn5mqjeaheeuew2gjs785f6k7jm8vfsqg3jhtpkwppcmzqdk2xf9";
const WASM_CONTRACT: &str = "lom1szrfu43ncn6as3mgjd8davelgd77zdj7n3zhwkuc8w85gc3yrctsdrnnxl";
const IBC_CHANNEL: &str = "channel-0";
const RECEIVER: &str = "cosmos1qh44ugsak6mejr60dsew5f0vaaxpl0pqhtzs2pudl37hkukk09aqjm5ex2";
const RECOVER_ADDRESS: &str = "lom1g8p66wfxmvvknv5w23ntxsl9wj8rr4923zfquk8tw8kemrlz8rks8m7fn7";

/// Main circuit function for token transfer validation
pub fn circuit(witnesses: Vec<Witness>) -> Vec<u8> {
    // Ensure we have the expected number of witnesses
    assert_eq!(
        witnesses.len(),
        4,
        "Expected 4 witnesses: fee amount, fee receiver, fee and memo"
    );

    // Extract witness data
    let fee_amount_bytes = witnesses[0].as_data().expect("Failed to get fee amount");
    let fee_recipient_bytes = witnesses[1].as_data().expect("Failed to get route data");
    let fee_expiration_bytes = witnesses[2]
        .as_data()
        .expect("Failed to get fee expiration");
    let memo = witnesses[3].as_data().expect("Failed to get memo data");

    // Parse fee amount
    let fee_amount = u64::from_le_bytes(
        <[u8; 8]>::try_from(fee_amount_bytes).expect("Fee data must be exactly 8 bytes"),
    );

    if fee_amount > MAX_FEE_ALLOWED {
        panic!(
            "Fee amount exceeds the maximum allowed limit of {}",
            MAX_FEE_ALLOWED
        );
    }

    // Parse fee recipient
    let fee_recipient =
        core::str::from_utf8(fee_recipient_bytes).expect("Fee recipient data must be valid UTF-8");

    // Parse fee expiration
    let fee_expiration = u64::from_le_bytes(
        <[u8; 8]>::try_from(fee_expiration_bytes).expect("Expiration data must be exactly 8 bytes"),
    );

    // Parse get the memo
    let memo: Value = serde_json::from_slice(memo).unwrap();

    // Validate the memo
    validate_memo(&memo);

    // Generate ZkMessage
    let zk_message = generate_zk_message(
        fee_amount,
        fee_recipient.to_string(),
        fee_expiration,
        memo.to_string(),
    );

    // Return ABI-encoded ZkMessage
    zk_message.abi_encode()
}

// Define Valence contract types using alloy-sol-types
sol! {
    /// Duration type for Valence messages
    enum DurationType {
        Height,
        Time
    }

    /// Duration structure
    struct Duration {
        DurationType durationType;
        uint64 value;
    }

    /// Retry times type
    enum RetryTimesType {
        NoRetry,
        Indefinitely,
        Amount
    }

    /// Retry times structure
    struct RetryTimes {
        RetryTimesType retryType;
        uint64 amount;
    }

    /// Retry logic structure
    struct RetryLogic {
        RetryTimes times;
        Duration interval;
    }

    /// Atomic function structure
    struct AtomicFunction {
        address contractAddress;
    }

    /// Atomic subroutine structure
    struct AtomicSubroutine {
        AtomicFunction[] functions;
        RetryLogic retryLogic;
    }

    /// Subroutine type
    enum SubroutineType {
        Atomic,
        NonAtomic
    }

    /// Subroutine structure
    struct Subroutine {
        SubroutineType subroutineType;
        bytes subroutine;
    }

    /// Priority enum
    enum Priority {
        Medium,
        High
    }

    /// SendMsgs structure
    struct SendMsgs {
        uint64 executionId;
        Priority priority;
        Subroutine subroutine;
        uint64 expirationTime;
        bytes[] messages;
    }

    /// ProcessorMessage type enum
    enum ProcessorMessageType {
        Pause,
        Resume,
        EvictMsgs,
        SendMsgs,
        InsertMsgs
    }

    /// ProcessorMessage structure
    struct ProcessorMessage {
        ProcessorMessageType messageType;
        bytes message;
    }

    /// ZkMessage structure for Valence Authorization
    struct ZkMessage {
        uint64 registry;
        uint64 blockNumber;
        address authorizationContract;
        ProcessorMessage processorMessage;
    }

    /// Fees structure for IBC Eureka transfer
    struct Fees {
        uint256 relayFee;
        address relayFeeRecipient;
        uint64 quoteExpiry;
    }

    /// lombardTransfer function call for IBC Eureka transfer library
    function lombardTransfer(Fees calldata fees, string calldata memo) external;
}

fn validate_memo(memo: &Value) {
    // Validate dest_callback
    let dest_callback = memo
        .get("dest_callback")
        .and_then(|dc| dc.get("address"))
        .and_then(|addr| addr.as_str())
        .expect("dest_callback address not found");

    if dest_callback != DEST_CALLBACK {
        panic!("Invalid dest_callback address: {}", dest_callback);
    }

    // Validate wasm contract
    let wasm_contract = memo
        .get("wasm")
        .and_then(|w| w.get("contract"))
        .and_then(|c| c.as_str())
        .unwrap();

    if wasm_contract != WASM_CONTRACT {
        panic!("Invalid wasm contract address: {}", wasm_contract);
    }

    // Navigate to ibc_info more safely
    let ibc_info = memo
        .get("wasm")
        .and_then(|w| w.get("msg"))
        .and_then(|m| m.get("swap_and_action"))
        .and_then(|sa| sa.get("post_swap_action"))
        .and_then(|psa| psa.get("ibc_transfer"))
        .and_then(|it| it.get("ibc_info"))
        .expect("ibc_info not found");

    // Validate IBC channel
    let ibc_channel = ibc_info
        .get("source_channel")
        .and_then(|sc| sc.as_str())
        .expect("source_channel not found");

    if ibc_channel != IBC_CHANNEL {
        panic!("Invalid IBC channel: {}", ibc_channel);
    }

    // Validate receiver
    let receiver = ibc_info.get("receiver").and_then(|r| r.as_str()).unwrap();

    if receiver != RECEIVER {
        panic!("Invalid IBC receiver address: {}", receiver);
    }

    // Validate recover address
    let recover_address = ibc_info
        .get("recover_address")
        .and_then(|ra| ra.as_str())
        .expect("recover_address not found");

    if recover_address != RECOVER_ADDRESS {
        panic!("Invalid recover address: {}", recover_address);
    }
}

/// Generate ZkMessage for Valence Authorization contract
fn generate_zk_message(
    fee_amount: u64,
    fee_recipient: String,
    expiration: u64,
    memo: String,
) -> ZkMessage {
    // Create the Fees structure for the lombard transfer call
    let fees = Fees {
        relayFee: alloy_primitives::U256::from(fee_amount),
        relayFeeRecipient: fee_recipient.parse().unwrap(),
        quoteExpiry: expiration,
    };

    // Create the lombard transfer function call with validated fees and the validated memo
    let transfer_call = lombardTransferCall { fees, memo };

    // ABI encode the lombard transfer call
    let encoded_transfer_call = transfer_call.abi_encode();

    let atomic_function = AtomicFunction {
        contractAddress: EUREKA_TRANSFER_LIBRARY_CONTRACT.parse().unwrap(),
    };

    // Create retry logic with NoRetry for atomic execution
    let retry_logic = RetryLogic {
        times: RetryTimes {
            retryType: RetryTimesType::NoRetry,
            amount: 0,
        },
        interval: Duration {
            durationType: DurationType::Time,
            value: 0,
        },
    };

    // Create AtomicSubroutine
    let atomic_subroutine = AtomicSubroutine {
        functions: alloc::vec![atomic_function],
        retryLogic: retry_logic,
    };

    // Encode the atomic subroutine
    let encoded_subroutine = atomic_subroutine.abi_encode();

    // Create Subroutine wrapper
    let subroutine = Subroutine {
        subroutineType: SubroutineType::Atomic,
        subroutine: Bytes::from(encoded_subroutine),
    };

    // Create SendMsgs message with the properly encoded transfer call
    let send_msgs = SendMsgs {
        executionId: 1, // Generated execution ID
        priority: Priority::Medium,
        subroutine,
        expirationTime: 0, // No expiration
        messages: alloc::vec![Bytes::from(encoded_transfer_call)],
    };

    // Encode SendMsgs
    let encoded_send_msgs = send_msgs.abi_encode();

    // Create ProcessorMessage
    let processor_message = ProcessorMessage {
        messageType: ProcessorMessageType::SendMsgs,
        message: Bytes::from(encoded_send_msgs),
    };

    // Create final ZkMessage
    ZkMessage {
        registry: 0,                          // Same registry the authorization is created for
        blockNumber: 0,                       // We are not validating it
        authorizationContract: Address::ZERO, // Valid for any contract
        processorMessage: processor_message,
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    const MEMO: &str = r#"
    {
        "dest_callback": {
            "address": "lom13ehuhysn5mqjeaheeuew2gjs785f6k7jm8vfsqg3jhtpkwppcmzqdk2xf9"
        },
        "wasm": {
            "contract": "lom1szrfu43ncn6as3mgjd8davelgd77zdj7n3zhwkuc8w85gc3yrctsdrnnxl",
            "msg": {
                "swap_and_action": {
                    "post_swap_action": {
                        "ibc_transfer": {
                            "ibc_info": {
                                "source_channel": "channel-0",
                                "receiver": "cosmos14mlpd48k5vkeset4x7f78myz3m47jcax4mesvx",
                                "recover_address": "lom1g8p66wfxmvvknv5w23ntxsl9wj8rr4923zfquk8tw8kemrlz8rks8m7fn7"
                            }
                        }
                    }
                }
            }
        }
    }"#;

    #[test]
    fn test_circuit_valid_transfer() {
        let fee_amount = 957u64; // Valid fee below threshold
        let receiver = "0x33C4DaD158F1E2cCF97bF17d1574d5b7b9f43002";
        let expiration = 1890000000000000u64; // Example expiration timestamp

        let witnesses = vec![
            Witness::Data(fee_amount.to_le_bytes().to_vec()),
            Witness::Data(receiver.as_bytes().to_vec()),
            Witness::Data(expiration.to_le_bytes().to_vec()),
            Witness::Data(MEMO.as_bytes().to_vec()), // Memo data
        ];

        let result = circuit(witnesses);

        // Try to decode the ZkMessage to verify it's valid
        let decoded_result = ZkMessage::abi_decode(&result, false);
        assert!(decoded_result.is_ok(), "Should be able to decode ZkMessage");
    }

    #[test]
    #[should_panic(expected = "Fee amount exceeds the maximum allowed limit of 1930")]
    fn test_circuit_excessive_fees() {
        let fee_amount = 2000u64; // Exceeds the maximum allowed fee
        let receiver = "0x33C4DaD158F1E2cCF97bF17d1574d5b7b9f43002";
        let expiration = 1890000000000000u64; // Example expiration timestamp

        let witnesses = vec![
            Witness::Data(fee_amount.to_le_bytes().to_vec()),
            Witness::Data(receiver.as_bytes().to_vec()),
            Witness::Data(expiration.to_le_bytes().to_vec()),
            Witness::Data(MEMO.as_bytes().to_vec()), // Memo data
        ];

        circuit(witnesses);
    }

    #[test]
    #[should_panic(expected = "Invalid dest_callback address: lom123")]
    fn test_invalid_dest_callback_address() {
        let fee_amount = 900u64;
        let receiver = "0x33C4DaD158F1E2cCF97bF17d1574d5b7b9f43002";
        let expiration = 1890000000000000u64; // Example expiration timestamp
        let invalid_memo = MEMO.replace(
            "lom13ehuhysn5mqjeaheeuew2gjs785f6k7jm8vfsqg3jhtpkwppcmzqdk2xf9",
            "lom123", // Invalid dest_callback address
        );

        let witnesses = vec![
            Witness::Data(fee_amount.to_le_bytes().to_vec()),
            Witness::Data(receiver.as_bytes().to_vec()),
            Witness::Data(expiration.to_le_bytes().to_vec()),
            Witness::Data(invalid_memo.as_bytes().to_vec()), // Memo data
        ];

        circuit(witnesses);
    }
}
