use alloy::sol;

sol!(
    #[sol(rpc)]
    ERC1967Proxy,
    "./../deploy/contracts/evm/ERC1967Proxy.sol/ERC1967Proxy.json",
);

sol!(
    #[sol(rpc)]
    BaseAccount,
    "./../deploy/contracts/evm/BaseAccount.sol/BaseAccount.json",
);

sol!(
    #[sol(rpc)]
    ERC20,
    "./../deploy/contracts/evm/ERC20.sol/ERC20.json",
);

// Need to use a module to avoid name conflicts with Authorization
pub mod processor_contract {
    alloy::sol!(
        #[sol(rpc)]
        LiteProcessor,
        "./../deploy/contracts/evm/LiteProcessor.sol/LiteProcessor.json",
    );
}

sol!(
    #[sol(rpc)]
    Authorization,
    "./../deploy/contracts/evm/Authorization.sol/Authorization.json",
);

sol!(
    #[sol(rpc)]
    #[derive(Debug, PartialEq, Eq)]
    OneWayVault,
    "./../deploy/contracts/evm/OneWayVault.sol/OneWayVault.json",
);

sol!(
    #[sol(rpc)]
    IBCEurekaTransfer,
    "./../deploy/contracts/evm/IBCEurekaTransfer.sol/IBCEurekaTransfer.json",
);
