use anyhow::anyhow;
use serde_json::Value;

pub fn get_amount_out(json: &Value) -> anyhow::Result<u128> {
    let amount_out_str = json
        .get("amount_out")
        .and_then(Value::as_str)
        .ok_or(anyhow!(
            "skip_api response amount_out not found or not a string"
        ))?;

    let amount_out_u128: u128 = amount_out_str.parse()?;

    Ok(amount_out_u128)
}
