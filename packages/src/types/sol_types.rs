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
    SP1VerificationGateway,
    "src/contracts/evm/SP1VerificationGateway.sol/SP1VerificationGateway.json",
);
