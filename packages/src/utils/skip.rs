use serde_json::Value;
use anyhow::anyhow;

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

pub fn get_operations_array(json: &Value) -> anyhow::Result<Vec<Value>> {
    let skip_response_operations = json
        .get("operations")
        .cloned()
        .ok_or(anyhow!("failed to get operations"))?
        .as_array()
        .cloned()
        .ok_or(anyhow!("operations not an array"))?;

    Ok(skip_response_operations)
}

pub fn get_eureka_transfer_operation(json: Vec<Value>) -> anyhow::Result<Value> {
    let skip_response_eureka_operation = json
        .iter()
        .find(|op| op.get("eureka_transfer").cloned().is_some())
        .ok_or(anyhow!("no eureka transfer operation in skip response"))?;

    Ok(skip_response_eureka_operation.clone())
}
