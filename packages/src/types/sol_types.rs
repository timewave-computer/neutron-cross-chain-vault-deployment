use alloy::sol;

sol!(
    #[sol(rpc)]
    ERC1967Proxy,
    "src/contracts/evm/ERC1967Proxy.sol/ERC1967Proxy.json",
);

sol!(
    #[sol(rpc)]
    BaseAccount,
    "src/contracts/evm/BaseAccount.sol/BaseAccount.json",
);

sol!(
    #[sol(rpc)]
    ERC20,
    "src/contracts/evm/ERC20.sol/ERC20.json",
);

// Need to use a module to avoid name conflicts with Authorization
pub mod processor_contract {
    alloy::sol!(
        #[sol(rpc)]
        LiteProcessor,
        "src/contracts/evm/LiteProcessor.sol/LiteProcessor.json",
    );
}

sol!(
    #[sol(rpc)]
    Authorization,
    "src/contracts/evm/Authorization.sol/Authorization.json",
);

sol!(
    #[sol(rpc)]
    #[derive(Debug, PartialEq, Eq)]
    OneWayVault,
    "src/contracts/evm/OneWayVault.sol/OneWayVault.json",
);

sol!(
    #[sol(rpc)]
    IBCEurekaTransfer,
    "src/contracts/evm/IBCEurekaTransfer.sol/IBCEurekaTransfer.json",
);

sol!(
    #[sol(rpc)]
    CCTPTransfer,
    "src/contracts/evm/CCTPTransfer.sol/CCTPTransfer.json",
);

sol!(
    #[sol(rpc)]
    SP1VerificationGateway,
    "src/contracts/evm/SP1VerificationGateway.sol/SP1VerificationGateway.json",
);

sol!(
    struct IBCEurekaTransferConfig {
        uint256 amount;
        uint256 minAmountOut;
        address transferToken;
        address inputAccount;
        string recipient;
        string sourceClient;
        uint64 timeout;
        address eurekaHandler;
    }

    struct CCTPTransferConfig {
        uint256 amount;
        bytes32 mintRecipient;
        address inputAccount;
        uint32 destinationDomain;
        address cctpTokenMessenger;
        address transferToken;
    }
);

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
}
