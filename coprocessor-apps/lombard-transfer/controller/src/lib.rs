//! Token IBC Eureka Transfer Controller - purpose of this file is to handle witness generation and validation logic for token transfers

#![no_std]

extern crate alloc;

use alloc::{
    string::{String, ToString as _},
    vec::Vec,
};
use chrono::{DateTime, Utc};
use serde_json::Value;
use valence_coprocessor::Witness;
use valence_coprocessor_wasm::abi;

#[derive(Debug)]
struct FeeData {
    amount: u64,
    recipient: String,
    expiration: u64,
}

/// Extract fee information from Skip API response
fn extract_fee_data(skip_response: &Value) -> anyhow::Result<FeeData> {
    abi::log!("Extracting fee data from Skip API response")?;

    // Extract the fees where all the information is provided
    let smart_relay_fees =
        &skip_response["operations"][0]["eureka_transfer"]["smart_relay_fee_quote"];

    let fee_amount = smart_relay_fees["fee_amount"].as_str().unwrap();

    let expiration = smart_relay_fees["expiration"].as_str().unwrap();

    // Expiration is in ISO 8601 so we convert using chrono
    let expiration_timestamp = DateTime::parse_from_rfc3339(expiration)
        .map_err(|_| anyhow::anyhow!("Can't parse the expiration timestamp"))?
        .with_timezone(&Utc)
        .timestamp() as u64;

    let fee_receiver = smart_relay_fees["fee_payment_address"].as_str().unwrap();

    let fee_data = FeeData {
        amount: fee_amount
            .parse::<u64>()
            .map_err(|_| anyhow::anyhow!("Invalid fee amount in Skip API response"))?,
        recipient: fee_receiver.to_string(),
        expiration: expiration_timestamp,
    };

    abi::log!("Fees extracted: {:?}", fee_data)?;
    Ok(fee_data)
}

pub fn get_witnesses(args: Value) -> anyhow::Result<Vec<Witness>> {
    abi::log!(
        "received a proof request for eureka transfer arguments {}",
        serde_json::to_string(&args).unwrap_or_default()
    )?;

    // Get Skip response from args. Format is { "skip_response": { ... } }
    let skip_response = args.get("skip_response").unwrap();

    // Extract fee data
    let fee_data = extract_fee_data(skip_response)?;

    // Get memo from args. Format is { "memo": {...} }
    let memo = args.get("memo").unwrap();

    abi::log!("Fee data extracted: {:?}", fee_data)?;

    // Prepare witness data for circuit (3 witnesses expected)
    let witnesses = [
        Witness::Data(fee_data.amount.to_le_bytes().to_vec()), // Witness 0: Fee amount
        Witness::Data(fee_data.recipient.as_bytes().to_vec()), // Witness 1: Fee recipient address
        Witness::Data(fee_data.expiration.to_le_bytes().to_vec()), // Witness 2: Expiration timestamp
        Witness::Data(memo.to_string().as_bytes().to_vec()),       // Witness 3: Memo
    ]
    .to_vec();

    Ok(witnesses)
}

pub fn entrypoint(args: Value) -> anyhow::Result<Value> {
    abi::log!(
        "received an entrypoint request with arguments {}",
        serde_json::to_string(&args).unwrap_or_default()
    )?;

    let cmd = args["payload"]["cmd"].as_str().unwrap();

    match cmd {
        "store" => {
            let path = args["payload"]["path"].as_str().unwrap().to_string();
            let bytes = serde_json::to_vec(&args).unwrap();

            abi::set_storage_file(&path, &bytes).unwrap();
        }

        _ => panic!("unknown entrypoint command"),
    }

    Ok(args)
}

/// Generate mock Skip API response for testing
pub fn generate_mock_skip_response(fees_amount: &str) -> Value {
    serde_json::json!({
        "source_asset_denom": "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599",
        "source_asset_chain_id": "1",
        "dest_asset_denom": "ibc/D742E8566B0B8CC8F569D950051C09CF57988A88F0E45574BFB3079D41DE6462",
        "dest_asset_chain_id": "cosmoshub-4",
        "amount_in": "40000",
        "amount_out": "39803",
        "operations": [
            {
                "eureka_transfer": {
                    "destination_port": "transfer",
                    "source_client": "cosmoshub-0",
                    "from_chain_id": "1",
                    "to_chain_id": "cosmoshub-4",
                    "pfm_enabled": false,
                    "supports_memo": true,
                    "denom_in": "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599",
                    "denom_out": "ibc/D742E8566B0B8CC8F569D950051C09CF57988A88F0E45574BFB3079D41DE6462",
                    "entry_contract_address": "0xFc2d0487A0ae42ae7329a80dc269916A9184cF7C",
                    "callback_adapter_contract_address": "cosmos1lqu9662kd4my6dww4gzp3730vew0gkwe0nl9ztjh0n5da0a8zc4swsvd22",
                    "bridge_id": "EUREKA",
                    "smart_relay": true,
                    "smart_relay_fee_quote": {
                        "fee_amount": fees_amount,
                        "relayer_address": "",
                        "expiration": "2025-06-05T23:03:07Z",
                        "fee_denom": "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599",
                        "fee_payment_address": "0x33C4DaD158F1E2cCF97bF17d1574d5b7b9f43002"
                    },
                    "to_chain_callback_contract_address": "cosmos1lqu9662kd4my6dww4gzp3730vew0gkwe0nl9ztjh0n5da0a8zc4swsvd22",
                    "to_chain_entry_contract_address": "cosmos1clswlqlfm8gpn7n5wu0ypu0ugaj36urlhj7yz30hn7v7mkcm2tuqy9f8s5"
                },
                "tx_index": 0,
                "amount_in": "40000",
                "amount_out": "39803"
            }
        ],
        "chain_ids": [
            "1",
            "cosmoshub-4"
        ],
        "does_swap": false,
        "estimated_amount_out": "39803",
        "swap_venues": [],
        "txs_required": 1,
        "usd_amount_in": "40.57",
        "usd_amount_out": "40.37",
        "estimated_fees": [
            {
                "fee_type": "SMART_RELAY",
                "bridge_id": "EUREKA",
                "amount": "197",
                "usd_amount": "0.20",
                "origin_asset": {
                    "denom": "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599",
                    "chain_id": "1",
                    "origin_denom": "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599",
                    "origin_chain_id": "1",
                    "trace": "",
                    "is_cw20": false,
                    "is_evm": false,
                    "is_svm": false,
                    "symbol": "WBTC",
                    "name": "Wrapped BTC",
                    "logo_uri": "https://raw.githubusercontent.com/axelarnetwork/axelar-configs/main/images/tokens/wbtc.svg",
                    "decimals": 8,
                    "token_contract": "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599",
                    "description": "",
                    "coingecko_id": "wrapped-bitcoin",
                    "recommended_symbol": "WBTC"
                },
                "chain_id": "1",
                "tx_index": 0
            }
        ],
        "required_chain_addresses": [
            "1",
            "cosmoshub-4"
        ],
        "estimated_route_duration_seconds": 1200
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_fee_data() {
        let mock_response = generate_mock_skip_response("200");
        let fees = extract_fee_data(&mock_response).unwrap();

        assert_eq!(fees.amount, 200);
        assert_eq!(fees.recipient, "0x33C4DaD158F1E2cCF97bF17d1574d5b7b9f43002");
        assert_eq!(fees.expiration, 1749164587);
    }
}
