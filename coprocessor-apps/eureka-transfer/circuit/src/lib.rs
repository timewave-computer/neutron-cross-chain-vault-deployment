#![no_std]

extern crate alloc;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use alloy_primitives::{Address, Bytes};
use alloy_sol_types::{SolCall, SolValue, sol};
use valence_coprocessor::Witness;

// Currently fee is 0.20$ which translates currently to 193. So we'll set a ceiling of 10 times that.
const MAX_FEE_ALLOWED: u64 = 1930;
// The library this will be executed on:
const EUREKA_TRANSFER_LIBRARY_CONTRACT: &str = "0xc8A8ADc4B612EbE10d239955D35640d80748CDB3";

/// Main circuit function for token transfer validation
pub fn circuit(witnesses: Vec<Witness>) -> Vec<u8> {
    // Ensure we have the expected number of witnesses
    assert_eq!(
        witnesses.len(),
        3,
        "Expected 3 witnesses: fee amount, fee receiver and fee "
    );

    // Extract witness data
    let fee_amount_bytes = witnesses[0].as_data().expect("Failed to get fee amount");
    let fee_recipient_bytes = witnesses[1].as_data().expect("Failed to get route data");
    let fee_expiration_bytes = witnesses[2]
        .as_data()
        .expect("Failed to get fee expiration");

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

    // Generate ZkMessage
    let zk_message = generate_zk_message(fee_amount, fee_recipient.to_string(), fee_expiration);

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

    /// Transfer function call for IBC Eureka transfer
    function transfer(Fees calldata fees, string calldata memo) external;
}

/// Generate ZkMessage for Valence Authorization contract
fn generate_zk_message(fee_amount: u64, fee_recipient: String, expiration: u64) -> ZkMessage {
    // Create the Fees structure for the transfer call
    let fees = Fees {
        relayFee: alloy_primitives::U256::from(fee_amount),
        relayFeeRecipient: fee_recipient.parse().unwrap(),
        quoteExpiry: expiration,
    };

    // Create the transfer function call with validated fees and empty memo
    let transfer_call = transferCall {
        fees,
        memo: String::new(), // Empty memo as required
    };

    // ABI encode the transfer call
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

    #[test]
    fn test_circuit_valid_transfer() {
        let fee_amount = 957u64; // Valid fee below threshold
        let receiver = "0x33C4DaD158F1E2cCF97bF17d1574d5b7b9f43002";
        let expiration = 1890000000000000u64; // Example expiration timestamp

        let witnesses = vec![
            Witness::Data(fee_amount.to_le_bytes().to_vec()),
            Witness::Data(receiver.as_bytes().to_vec()),
            Witness::Data(expiration.to_le_bytes().to_vec()),
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
        ];

        circuit(witnesses);
    }
}
