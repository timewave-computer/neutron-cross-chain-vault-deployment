use cosmwasm_std::{Decimal, Uint128};
use valence_domain_clients::{clients::neutron::NeutronClient, cosmos::wasm_client::WasmClient};

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
enum QueryMsg {
    SimulateDeposit { amount: Uint128 },
    ExchangeRate {},
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
struct SimulateDepositResponse {
    minted_amount: Uint128,
}

pub async fn query_maxbtc_simulate_deposit(
    client: &NeutronClient,
    maxbtc_contract: &str,
    amount: u128,
) -> anyhow::Result<u128> {
    let simulate_deposit = QueryMsg::SimulateDeposit {
        amount: Uint128::new(amount),
    };

    let response: SimulateDepositResponse = client
        .query_contract_state(maxbtc_contract, simulate_deposit)
        .await?;

    Ok(response.minted_amount.u128())
}

pub async fn query_maxbtc_er(
    client: &NeutronClient,
    maxbtc_contract: &str,
) -> anyhow::Result<Decimal> {
    let simulate_deposit = QueryMsg::ExchangeRate {};

    let response: Decimal = client
        .query_contract_state(maxbtc_contract, simulate_deposit)
        .await?;

    Ok(response)
}
