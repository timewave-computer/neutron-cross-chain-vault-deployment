use std::{env, error::Error, time::SystemTime};

use alloy::{
    hex::FromHex,
    primitives::{Address, Bytes, FixedBytes, U256},
};
use serde_json::{json, Value};
use sp1_sdk::{HashableKey, SP1VerifyingKey};
use types::sol_types::{
    processor_contract::LiteProcessor, Authorization, ERC1967Proxy, IBCEurekaTransfer,
    SP1VerificationGateway, ERC20,
};
use valence_domain_clients::{
    clients::{coprocessor::CoprocessorClient, ethereum::EthereumClient},
    coprocessor::base_client::CoprocessorBaseClient,
    evm::{
        anvil::AnvilImpersonationClient, base_client::EvmBaseClient,
        request_provider_client::RequestProviderClient,
    },
};

// Deployed this on mainnet for test purposes so you can fork anvil from mainnet
const IBC_EUREKA_LIBRARY: &str = "0xc8A8ADc4B612EbE10d239955D35640d80748CDB3";
const DEPOSIT_ACCOUNT: &str = "0xb129544624b79D58968eBb0090FC76374d072137";
// me lol
const OWNER_ACCOUNT: &str = "0xd9A23b58e684B985F661Ce7005AA8E10630150c1";

const SP1_VERIFIER: &str = "0x397A5f7f3dBd538f23DE225B51f532c34448dA9B";

const PROGRAM_ID: &str = "e6cde4039d5445dbce4de54f4504805356dc871f906cc46946181aa84754fd46";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv::dotenv().ok();
    let mnemonic = env::var("MNEMONIC").expect("mnemonic must be provided");

    let eth_client = EthereumClient::new("http://localhost:8545", &mnemonic, None)?;
    let rp = eth_client.get_request_provider().await?;
    let my_address = eth_client.signer().address();

    // Let's deploy a processor and authorization contract
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
    let verification_gateway = SP1VerificationGateway::new(verification_gateway_address, &rp);
    let initialize_verification_gateway_tx = verification_gateway
        .initialize(
            "0x0000000000000000000000000000000000000000000000000000000000000000"
                .parse()
                .unwrap(),
            SP1_VERIFIER.parse().unwrap(),
        )
        .into_transaction_request();
    eth_client
        .sign_and_send(initialize_verification_gateway_tx)
        .await?;
    println!("Verification Gateway initialized");

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

    let ibc_eureka_transfer = IBCEurekaTransfer::new(IBC_EUREKA_LIBRARY.parse().unwrap(), &rp);
    // We're going to update the prcessor to use this already deployed library
    let update_processor_tx = ibc_eureka_transfer
        .updateProcessor(processor_address)
        .into_transaction_request();
    eth_client
        .execute_tx_as(OWNER_ACCOUNT, update_processor_tx)
        .await?;
    println!("Processor updated with IBC Eureka library");

    // Let's fund the deposit account with some WBTC
    let wbtc = ERC20::new(
        "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599"
            .parse()
            .unwrap(),
        &rp,
    );
    let fund_deposit_account_tx = wbtc
        .transfer(DEPOSIT_ACCOUNT.parse().unwrap(), U256::from(1000))
        .into_transaction_request();
    eth_client
        .execute_tx_as(OWNER_ACCOUNT, fund_deposit_account_tx)
        .await?;
    println!("Deposit account funded with WBTC");

    // Now we can create the authorization that will trigger the transfer
    let coprocessor_client = CoprocessorClient::default();
    let program_vk = coprocessor_client.get_vk(PROGRAM_ID).await?;
    let domain_vk = coprocessor_client.get_domain_vk().await?;

    let sp1_program_vk: SP1VerifyingKey = bincode::deserialize(&program_vk)?;
    let sp1_domain_vk: SP1VerifyingKey = bincode::deserialize(&domain_vk)?;

    let authorization = Authorization::new(authorization, &rp);
    let registries = vec![0]; // Only one and IBC Eureka app will use registry 0
    let authorized_addresses = vec![Address::ZERO];
    let vks = vec![FixedBytes::<32>::from_hex(sp1_program_vk.bytes32()).unwrap()]; // Program verification key
    let domain_vk = FixedBytes::<32>::from_hex(sp1_domain_vk.bytes32()).unwrap(); // Domain verification key

    // Remember we send arrays because we allow  multiple registries added at once
    let tx = authorization
        .addRegistries(
            registries,
            vec![authorized_addresses],
            vks,
            domain_vk,
            vec![false],
        )
        .into_transaction_request();

    // Send the transaction
    eth_client.sign_and_send(tx).await?;
    println!("Authorization created successfully");

    let client = reqwest::Client::new();

    let payload = json!({
        "source_asset_denom": "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599",
        "source_asset_chain_id": "1",
        "dest_asset_denom": "ibc/D742E8566B0B8CC8F569D950051C09CF57988A88F0E45574BFB3079D41DE6462",
        "dest_asset_chain_id": "cosmoshub-4",
        "amount_in": "20000000",
        "allow_multi_tx": true,
        "allow_unsafe": true,
        "go_fast": true,
        "smart_relay": true,
        "experimental_features": ["eureka"],
        "smart_swap_options": {
            "split_routes": true,
            "evm_swaps": true
        }
    });

    let response = client
        .post("https://go.skip.build/api/skip/v2/fungible/route")
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?
        .text()
        .await?;

    let response_json: Value = serde_json::from_str(&response)?;

    let now = SystemTime::now();

    let proof = coprocessor_client
        .prove(PROGRAM_ID, &json!({"skip_response": response_json}))
        .await?;

    println!("Proof received!");

    let after = SystemTime::now();
    let duration = after.duration_since(now).unwrap();
    println!("Proof generation took: {:?}", duration);

    let (proof_program, inputs_program) = proof.program.decode()?;
    let (proof_domain, inputs_domain) = proof.domain.decode()?;
    // These should be different but for now just test with the same VK to verify it works
    let execute_tx = authorization
        .executeZKMessage(
            Bytes::from(inputs_program),
            Bytes::from(proof_program),
            Bytes::from(inputs_domain),
            Bytes::from(proof_domain),
        )
        .into_transaction_request();

    eth_client.sign_and_send(execute_tx).await?;

    // Verify that message was executed by checking accounts doesn't have funds anymore
    let deposit_account_balance = wbtc
        .balanceOf(DEPOSIT_ACCOUNT.parse().unwrap())
        .call()
        .await?;
    assert_eq!(deposit_account_balance._0, U256::from(0));

    println!("Transfer executed successfully");

    // Also check that we got the callback
    let callback = authorization.callbacks(0).call().await?;

    assert_eq!(callback.executionResult, 0); // 0 means success
    println!("Executed succesfully!");
    Ok(())
}
