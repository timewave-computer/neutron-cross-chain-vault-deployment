#![no_std]

extern crate alloc;

use alloc::{format, string::ToString as _, vec::Vec};
use alloy_primitives::U256;
use alloy_sol_types::SolValue;
use ethereum_merkle_proofs::merkle_lib::digest_keccak;
use hex::encode;
use serde_json::{Value, json};
use valence_coprocessor::Witness;
use valence_coprocessor_wasm::abi::{self};

/// Mainnet RPC endpoint for Ethereum network
const MAINNET_RPC_URL: &str = "https://eth-mainnet.public.blastapi.io";
const VAULT_ADDRESS: &str = "0x58e3e5eeae41c4ab8f954189dc9daef8ac7a17f9";

pub fn get_witnesses(args: Value) -> anyhow::Result<Vec<Witness>> {
    let withdraw_request_id = args["withdraw_request_id"].as_u64().unwrap();

    let rpc_request = build_eth_call_request(withdraw_request_id, VAULT_ADDRESS, MAINNET_RPC_URL);
    let http_response = abi::http(&rpc_request)?;

    let body_bytes: Vec<u8> = http_response["body"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No body in HTTP response"))?
        .iter()
        .map(|v| v.as_u64().unwrap_or(0) as u8)
        .collect();

    let json_rpc_response: Value = serde_json::from_slice(&body_bytes)?;
    let response_result = json_rpc_response["result"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No result in JSON-RPC response"))?;

    let hex_data = response_result
        .strip_prefix("0x")
        .ok_or_else(|| anyhow::anyhow!("Invalid hex format"))?;

    let bytes =
        hex::decode(hex_data).map_err(|e| anyhow::anyhow!("Failed to decode hex: {}", e))?;

    let decoded = <(u64, alloy_primitives::Address, U256, U256, U256)>::abi_decode(&bytes, false)?;

    let string_offset = decoded.4.to::<usize>();
    let string_length = u32::from_be_bytes([
        bytes[string_offset + 28],
        bytes[string_offset + 29],
        bytes[string_offset + 30],
        bytes[string_offset + 31],
    ]) as usize;
    let string_data = &bytes[string_offset + 32..string_offset + 32 + string_length];
    let receiver = alloc::string::String::from_utf8(string_data.to_vec())?;

    let withdraw_request_id_bytes = decoded.0.to_le_bytes().to_vec();
    let withdraw_request_redemption_rate_bytes = decoded.2.to_le_bytes_vec();
    let withdraw_request_shares_amount_bytes = decoded.3.to_le_bytes_vec();
    let recipient_bytes = receiver.as_bytes().to_vec();

    let witnesses = [
        Witness::Data(withdraw_request_id_bytes),
        Witness::Data(withdraw_request_shares_amount_bytes),
        Witness::Data(withdraw_request_redemption_rate_bytes),
        Witness::Data(recipient_bytes),
    ]
    .to_vec();

    Ok(witnesses)
}

pub fn build_eth_call_request(
    withdraw_request_id: u64,
    vault_address: &str,
    rpc_url: &str,
) -> Value {
    let function_sig = "withdrawRequests(uint64)";
    let function_selector = &digest_keccak(function_sig.as_bytes())[0..4];
    let encoded_params = (withdraw_request_id).abi_encode();
    let call_data = [function_selector, &encoded_params].concat();
    let call_data_hex = encode(call_data);

    let rpc_call = json!({
        "jsonrpc": "2.0",
        "method": "eth_call",
        "params": [{
            "to": vault_address,
            "data": format!("0x{}", call_data_hex)
        }, "latest"],
        "id": 1
    });

    json!({
        "url": rpc_url,
        "method": "POST",
        "headers": {
            "Content-Type": "application/json"
        },
        "body": rpc_call.to_string()
    })
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_get_witnesses() {
        let result = "0x0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000d9a23b58e684b985f661ce7005aa8e10630150c100000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000005f5e100000000000000000000000000000000000000000000000000000000000000003200000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000426e657574726f6e316d32656d6339336d3967707767737273663276796c76397876677168363534363330763764667268726b6d7235736c6c79353373706738357776000000000000000000000000000000000000000000000000000000000000";

        let hex_data = result.strip_prefix("0x").unwrap();
        let bytes = hex::decode(hex_data).unwrap();

        // Test first field (works)
        let first_field = <(u64,)>::abi_decode(&bytes, false).unwrap();
        assert_eq!(first_field.0, 1);

        // Test first two fields
        let two_fields = <(u64, alloy_primitives::Address)>::abi_decode(&bytes, false).unwrap();
        assert_eq!(two_fields.0, 1);

        // Test first three fields
        let three_fields =
            <(u64, alloy_primitives::Address, bool)>::abi_decode(&bytes, false).unwrap();
        assert_eq!(three_fields.0, 1);
        assert!(three_fields.2);

        // Test first four fields
        let four_fields =
            <(u64, alloy_primitives::Address, bool, U256)>::abi_decode(&bytes, false).unwrap();
        assert_eq!(four_fields.0, 1);
        assert_eq!(four_fields.3, U256::from(100000000u32));

        // Test first five fields
        let five_fields =
            <(u64, alloy_primitives::Address, bool, U256, U256)>::abi_decode(&bytes, false)
                .unwrap();
        assert_eq!(five_fields.0, 1);
        assert_eq!(five_fields.4, U256::from(50u32));

        // Now test all six fields - this is where it probably fails
        let all_fields =
            <(u64, alloy_primitives::Address, bool, U256, U256, U256)>::abi_decode(&bytes, false)
                .unwrap();

        // Now we have the string offset in decoded.5
        let string_offset = all_fields.5.to::<usize>();

        // Extract string manually using the offset
        let string_length: usize = u32::from_be_bytes([
            bytes[string_offset + 28],
            bytes[string_offset + 29],
            bytes[string_offset + 30],
            bytes[string_offset + 31],
        ]) as usize;

        let string_data = &bytes[string_offset + 32..string_offset + 32 + string_length];
        let receiver = alloc::string::String::from_utf8(string_data.to_vec()).unwrap();

        assert_eq!(
            receiver,
            "neutron1m2emc93m9gpwgsrsf2vylv9xvgqh654630v7dfrhrkmr5slly53spg85wv"
        )
    }
}
