use std::{env, error::Error, fs};

use alloy::{
    hex::FromHex,
    primitives::{Address, Bytes, FixedBytes, U256},
    sol,
    sol_types::SolValue,
};
use cosmwasm_std::Uint128;
use serde::Deserialize;
use sp1_sdk::{HashableKey, SP1VerifyingKey};
use types::{
    ethereum_config::{
        EthereumAccounts, EthereumCoprocessorAppIds, EthereumDenoms, EthereumLibraries,
        EthereumStrategyConfig,
    },
    sol_types::{
        processor_contract::LiteProcessor,
        Authorization, BaseAccount, ERC1967Proxy, IBCEurekaTransfer,
        OneWayVault::{self, FeeDistributionConfig, OneWayVaultConfig},
        SP1VerificationGateway,
    },
};
use valence_domain_clients::{
    clients::{coprocessor::CoprocessorClient, ethereum::EthereumClient},
    coprocessor::base_client::CoprocessorBaseClient,
    evm::{base_client::EvmBaseClient, request_provider_client::RequestProviderClient},
};

#[derive(Deserialize, Debug)]
struct Parameters {
    general: General,
    vault: Vault,
    eureka_transfer: EurekaTransfer,
    coprocessor_app: CoprocessorApp,
}

#[derive(Deserialize, Debug)]
struct General {
    rpc_url: String,
    owner: Address,
    valence_owner: Address,
    coprocessor_root: String,
}

#[derive(Deserialize, Debug)]
struct Vault {
    deposit_token: Address,
    strategist: Address,
    platform_fee_account: Address,
    strategist_fee_account: Address,
    strategist_fee_ratio_bps: u32,
    scaling_factor: Uint128,
    deposit_cap: U256,
    deposit_fee_bps: u32,
    withdraw_rate_bps: u32,
    starting_rate: U256,
    max_rate_update_delay: u64,
}

#[derive(Deserialize, Debug)]
struct EurekaTransfer {
    handler: Address,
    recipient: String,
    source_client: String,
    timeout: u64,
}

#[derive(Deserialize, Debug)]
struct CoprocessorApp {
    eureka_transfer_coprocessor_app_id: String,
}

const SP1_VERIFIER: &str = "0x397A5f7f3dBd538f23DE225B51f532c34448dA9B";

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
        maxRateUpdateDelay: parameters.vault.max_rate_update_delay,
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

    let processor =
        LiteProcessor::deploy_builder(&rp, FixedBytes::<32>::default(), Address::ZERO, 0, vec![])
            .into_transaction_request();

    let processor_address = eth_client
        .sign_and_send(processor)
        .await?
        .contract_address
        .unwrap();
    println!("Processor deployed at: {processor_address}");

    let verification_gateway =
        SP1VerificationGateway::deploy_builder(&rp).into_transaction_request();
    let verification_gateway_implementation = eth_client
        .sign_and_send(verification_gateway)
        .await?
        .contract_address
        .unwrap();

    let proxy_tx =
        ERC1967Proxy::deploy_builder(&rp, verification_gateway_implementation, Bytes::new())
            .into_transaction_request();
    let verification_gateway_address = eth_client
        .sign_and_send(proxy_tx)
        .await?
        .contract_address
        .unwrap();
    println!("Verification Gateway deployed at: {verification_gateway_address}");

    // Initialize the verification gateway
    // We need to get the domain vk of the coprocessor
    let coprocessor_client = CoprocessorClient::default();
    let domain_vk = coprocessor_client.get_domain_vk().await?;
    let sp1_domain_vk: SP1VerifyingKey = bincode::deserialize(&domain_vk)?;
    let domain_vk = FixedBytes::<32>::from_hex(sp1_domain_vk.bytes32()).unwrap();

    let verification_gateway = SP1VerificationGateway::new(verification_gateway_address, &rp);
    let initialize_verification_gateway_tx = verification_gateway
        .initialize(
            parameters.general.coprocessor_root.parse().unwrap(),
            SP1_VERIFIER.parse().unwrap(),
            domain_vk,
        )
        .into_transaction_request();
    eth_client
        .sign_and_send(initialize_verification_gateway_tx)
        .await?;
    println!("Verification Gateway initialized");

    // Transfer the ownership of the verification gateway
    let transfer_ownership_tx = verification_gateway
        .transferOwnership(parameters.general.valence_owner)
        .into_transaction_request();
    eth_client.sign_and_send(transfer_ownership_tx).await?;
    println!(
        "Verification Gateway ownership transferred to: {}",
        parameters.general.valence_owner
    );

    let authorization = Authorization::deploy_builder(
        &rp,
        my_address, // We will be initial owners to eventually add the authorizations, then we need to transfer ownership
        processor_address,
        verification_gateway_address,
        true, // Store callbacks
    );

    let authorization = eth_client
        .sign_and_send(authorization.into_transaction_request())
        .await?
        .contract_address
        .unwrap();
    println!("Authorization deployed at: {authorization}");

    // Add authorization contract as an authorized address to the proccessor
    let processor = LiteProcessor::new(processor_address, &rp);

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
        deposit_token: parameters.vault.deposit_token,
    };

    let accounts = EthereumAccounts {
        deposit: deposit_account,
    };

    let libraries = EthereumLibraries {
        one_way_vault: proxy,
        eureka_transfer,
    };

    let coprocessor_app_ids = EthereumCoprocessorAppIds {
        ibc_eureka: parameters
            .coprocessor_app
            .eureka_transfer_coprocessor_app_id,
    };

    let eth_cfg = EthereumStrategyConfig {
        ibc_transfer_threshold_amt: U256::from(1_000_000),
        rate_scaling_factor: parameters.vault.scaling_factor,
        rpc_url: parameters.general.rpc_url,
        authorizations: authorization,
        processor: processor_address,
        denoms,
        accounts,
        libraries,
        coprocessor_app_ids,
    };

    println!("Ethereum Strategy Config created successfully");

    // Save the Ethereum Strategy Config to a toml file
    let eth_cfg_toml =
        toml::to_string(&eth_cfg).expect("Failed to serialize Ethereum Strategy Config");
    fs::write(
        current_dir.join("deploy/src/ethereum_strategy_config.toml"),
        eth_cfg_toml,
    )
    .expect("Failed to write Ethereum Strategy Config to file");

    Ok(())
}
