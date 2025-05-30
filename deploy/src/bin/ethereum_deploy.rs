use std::{env, error::Error, fs, str::FromStr};

use alloy::{
    primitives::{Address, Bytes, FixedBytes, U256},
    sol,
    sol_types::SolValue,
};
use serde::Deserialize;
use types::ethereum_config::{
    EthereumAccounts, EthereumDenoms, EthereumLibraries, EthereumStrategyConfig,
};
use valence_domain_clients::{
    clients::ethereum::EthereumClient,
    evm::{base_client::EvmBaseClient, request_provider_client::RequestProviderClient},
};
use OneWayVault::{FeeDistributionConfig, OneWayVaultConfig};

#[derive(Deserialize, Debug)]
struct Parameters {
    general: General,
    vault: Vault,
    eureka_transfer: EurekaTransfer,
}

#[derive(Deserialize, Debug)]
struct General {
    rpc_url: String,
    owner: Address,
}

#[derive(Deserialize, Debug)]
struct Vault {
    deposit_token: Address,
    strategist: Address,
    platform_fee_account: Address,
    strategist_fee_account: Address,
    strategist_fee_ratio_bps: u32,
    deposit_cap: U256,
    deposit_fee_bps: u32,
    withdraw_rate_bps: u32,
    starting_rate: U256,
}

#[derive(Deserialize, Debug)]
struct EurekaTransfer {
    handler: Address,
    recipient: String,
    source_client: String,
    timeout: u64,
}

sol!(
    #[sol(rpc)]
    BaseAccount,
    "./contracts/evm/BaseAccount.sol/BaseAccount.json",
);

// Need to use a module to avoid name conflicts with Authorization
mod processor_contract {
    alloy::sol!(
        #[sol(rpc)]
        LiteProcessor,
        "./contracts/evm/LiteProcessor.sol/LiteProcessor.json",
    );
}

sol!(
    #[sol(rpc)]
    Authorization,
    "./contracts/evm/Authorization.sol/Authorization.json",
);

sol!(
    #[sol(rpc)]
    OneWayVault,
    "./contracts/evm/OneWayVault.sol/OneWayVault.json",
);

sol!(
    #[sol(rpc)]
    IBCEurekaTransfer,
    "./contracts/evm/IBCEurekaTransfer.sol/IBCEurekaTransfer.json",
);

sol!(
    #[sol(rpc)]
    ERC1967Proxy,
    "./contracts/evm/ERC1967Proxy.sol/ERC1967Proxy.json",
);

const VERIFICATION_GATEWAY: &str = "0x397A5f7f3dBd538f23DE225B51f532c34448dA9B";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv::dotenv().ok();
    let mnemonic = env::var("MNEMONIC").expect("mnemonic must be provided");

    // Read ethereum.toml from the deploy directory
    let current_dir = env::current_dir()?;
    let parameters = fs::read_to_string(current_dir.join("deploy/src/ethereum.toml"))
        .expect("Failed to read file");

    // Parse the TOML into your struct
    let parameters: Parameters = toml::from_str(&parameters).expect("Failed to parse TOML");

    let eth_client = EthereumClient::new(&parameters.general.rpc_url, &mnemonic, None)?;
    let my_address = eth_client.signer().address();
    let rp = eth_client.get_request_provider().await?;

    let deposit_account_tx =
        BaseAccount::deploy_builder(&rp, my_address, vec![]).into_transaction_request();

    let deposit_account = eth_client
        .sign_and_send(deposit_account_tx)
        .await?
        .contract_address
        .unwrap();
    println!("Deposit account deployed at: {}", deposit_account);

    let fee_distribution_config = FeeDistributionConfig {
        strategistAccount: parameters.vault.strategist_fee_account,
        platformAccount: parameters.vault.platform_fee_account,
        strategistRatioBps: parameters.vault.strategist_fee_ratio_bps,
    };

    let one_way_vault_config = OneWayVaultConfig {
        depositAccount: deposit_account,
        strategist: parameters.vault.strategist,
        depositFeeBps: parameters.vault.deposit_fee_bps,
        withdrawRateBps: parameters.vault.withdraw_rate_bps,
        depositCap: parameters.vault.deposit_cap,
        feeDistribution: fee_distribution_config,
    };

    let implementation_tx = OneWayVault::deploy_builder(&rp).into_transaction_request();

    let implementation = eth_client
        .sign_and_send(implementation_tx)
        .await?
        .contract_address
        .unwrap();

    let proxy_tx =
        ERC1967Proxy::deploy_builder(&rp, implementation, Bytes::new()).into_transaction_request();

    let proxy = eth_client
        .sign_and_send(proxy_tx)
        .await?
        .contract_address
        .unwrap();

    println!("Vault deployed at: {proxy}");

    let vault = OneWayVault::new(proxy, &rp);

    let initialize_tx = vault
        .initialize(
            parameters.general.owner,
            one_way_vault_config.abi_encode().into(),
            parameters.vault.deposit_token,
            "Neutron-XChain-Vault".to_string(), // vault token name
            "nVault".to_string(),               // vault token symbol
            parameters.vault.starting_rate,
        )
        .into_transaction_request();
    eth_client.sign_and_send(initialize_tx).await?;
    println!("Vault initialized");

    let processor = processor_contract::LiteProcessor::deploy_builder(
        &rp,
        FixedBytes::<32>::default(),
        Address::ZERO,
        0,
        vec![],
    )
    .into_transaction_request();

    let processor_address = eth_client
        .sign_and_send(processor)
        .await?
        .contract_address
        .unwrap();
    println!("Processor deployed at: {processor_address}");

    let authorization = Authorization::deploy_builder(
        &rp,
        my_address, // We will be initial owners to eventually add the authorizations, then we need to transfer ownership
        processor_address,
        Address::from_str(VERIFICATION_GATEWAY).unwrap(),
        true, // Store callbacks
    );

    let authorization = eth_client
        .sign_and_send(authorization.into_transaction_request())
        .await?
        .contract_address
        .unwrap();
    println!("Authorization deployed at: {authorization}");

    // Add authorization contract as an authorized address to the proccessor
    let processor = processor_contract::LiteProcessor::new(processor_address, &rp);

    let add_authorization_tx = processor
        .addAuthorizedAddress(authorization)
        .into_transaction_request();

    eth_client.sign_and_send(add_authorization_tx).await?;
    println!("Authorization added to processor");

    // Deploy Eureka Transfer
    sol!(
        struct IBCEurekaTransferConfig {
            uint256 amount;
            address transferToken;
            address inputAccount;
            string recipient;
            string sourceClient;
            uint64 timeout;
            address eurekaHandler;
        }
    );

    let eureka_transfer_config = IBCEurekaTransferConfig {
        amount: U256::ZERO, // Full amount
        transferToken: parameters.vault.deposit_token,
        inputAccount: deposit_account,
        recipient: parameters.eureka_transfer.recipient,
        sourceClient: parameters.eureka_transfer.source_client,
        timeout: parameters.eureka_transfer.timeout,
        eurekaHandler: parameters.eureka_transfer.handler,
    };

    let eureka_transfer = IBCEurekaTransfer::deploy_builder(
        &rp,
        parameters.general.owner,
        processor_address,
        eureka_transfer_config.abi_encode().into(),
    );

    let eureka_transfer = eth_client
        .sign_and_send(eureka_transfer.into_transaction_request())
        .await?
        .contract_address
        .unwrap();
    println!("Eureka Transfer deployed at: {eureka_transfer}");

    // Approve this library from the deposit account
    let base_account = BaseAccount::new(deposit_account, &rp);
    let approve_library_tx = base_account
        .approveLibrary(eureka_transfer)
        .into_transaction_request();
    eth_client.sign_and_send(approve_library_tx).await?;
    println!("Eureka Transfer library approved from deposit account");

    // Transfer ownership of the deposit account to the owner
    let transfer_ownership_tx = base_account
        .transferOwnership(parameters.general.owner)
        .into_transaction_request();
    eth_client.sign_and_send(transfer_ownership_tx).await?;

    // Query to verify the ownership was transferred
    let new_owner = base_account.owner().call().await?._0;
    println!("Deposit account ownership transferred to: {new_owner}");
    assert_eq!(new_owner, parameters.general.owner);

    // Create the Ethereum Strategy Config
    let denoms = EthereumDenoms {
        deposit_token: parameters.vault.deposit_token.to_string(),
    };

    let accounts = EthereumAccounts {
        deposit: deposit_account.to_string(),
    };

    let libraries = EthereumLibraries {
        one_way_vault: proxy.to_string(),
        eureka_transfer: eureka_transfer.to_string(),
    };

    let eth_cfg = EthereumStrategyConfig {
        rpc_url: "<Set RPC here>".to_string(),
        mnemonic: "<This will be taken from env>.".to_string(),
        authorizations: authorization.to_string(),
        processor: processor_address.to_string(),
        denoms,
        accounts,
        libraries,
    };

    println!("Ethereum Strategy Config created successfully");
    
    // Save the Ethereum Strategy Config to a toml file
    let eth_cfg_toml = toml::to_string(&eth_cfg).expect("Failed to serialize Ethereum Strategy Config");
    fs::write(current_dir.join("deploy/src/ethereum_strategy_config.toml"), eth_cfg_toml)
        .expect("Failed to write Ethereum Strategy Config to file");

    Ok(())
}
