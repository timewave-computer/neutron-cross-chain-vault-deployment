use std::error::Error;

use async_trait::async_trait;
use valence_domain_clients::{clients::neutron::NeutronClient, cosmos::wasm_client::WasmClient};
use valence_lending_utils::mars::{Account, Positions, QueryMsg};

#[async_trait]
pub trait MarsLending {
    async fn query_mars_lending_denom_amount(
        client: &NeutronClient,
        credit_manager: &str,
        acc_owner: &str,
        denom: &str,
    ) -> Result<u128, Box<dyn Error + Send + Sync>> {
        let credit_accounts =
            Self::query_mars_credit_accounts(client, credit_manager, acc_owner).await?;

        // get the first credit account. while credit accounts are returned as a vec,
        // mars lending library should only ever create one credit account and re-use it
        // for all LP actions, so we get the [0]
        let first_credit_account = match credit_accounts.first() {
            Some(acc) => acc,
            None => return Err(format!("no credit account found for owner {acc_owner}").into()),
        };

        let active_positions = Self::query_mars_credit_account_positions(
            client,
            credit_manager,
            first_credit_account.id.to_string(),
        )
        .await?;

        // iterate over active lending positions until the target denom is found
        for lend in active_positions.lends {
            if lend.denom == denom {
                return Ok(lend.amount.u128());
            }
        }

        // if target denom was not among the active lending positions, we return 0
        Ok(0)
    }

    async fn query_mars_credit_accounts(
        client: &NeutronClient,
        credit_manager: &str,
        acc_owner: &str,
    ) -> Result<Vec<Account>, Box<dyn Error + Send + Sync>> {
        // query the mars credit account created and owned by the mars input account
        let mars_input_acc_credit_accounts: Vec<Account> = client
            .query_contract_state(
                credit_manager,
                QueryMsg::Accounts {
                    owner: acc_owner.to_string(),
                    start_after: None,
                    limit: None,
                },
            )
            .await?;

        Ok(mars_input_acc_credit_accounts)
    }

    async fn query_mars_credit_account_positions(
        client: &NeutronClient,
        credit_manager: &str,
        account_id: String,
    ) -> Result<Positions, Box<dyn Error + Send + Sync>> {
        // query mars positions owned by the credit account id
        let mars_positions_response: Positions = client
            .query_contract_state(credit_manager, QueryMsg::Positions { account_id })
            .await?;

        Ok(mars_positions_response)
    }
}
