use valence_domain_clients::clients::neutron::NeutronClient;

pub async fn query_maxbtc_exchange_amount(
    _client: &NeutronClient,
    _maxbtc_contract: &str,
    _amount: u128,
) -> anyhow::Result<u128> {
    // TODO: Implement the logic to query how much maxBTC we would get for depositing a certain amount into the maxBTC contract
    // This query is not available yet but will be soon
    Ok(0)
}
