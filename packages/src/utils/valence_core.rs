use std::cmp::Ordering;

use alloy::{
    primitives::{Address, U256},
    providers::Provider,
};
use cosmwasm_std::{Binary, Decimal};

use anyhow::anyhow;
use log::{debug, info, warn};
use valence_authorization_utils::msg::ProcessorMessage;
use valence_domain_clients::{
    clients::{ethereum::EthereumClient, neutron::NeutronClient},
    cosmos::{base_client::BaseClient, wasm_client::WasmClient},
    evm::base_client::{CustomProvider, EvmBaseClient},
};

use crate::{
    labels::REGISTER_OBLIGATION_LABEL,
    phases::{DEPOSIT_PHASE, REGISTRATION_PHASE, UPDATE_PHASE},
    types::sol_types::OneWayVault,
};

const ICA_CONTRACT_FUNDING_AMT: u128 = 200_000;

pub async fn enqueue_neutron(
    client: &NeutronClient,
    authorizations: &str,
    label: &str,
    messages: Vec<Binary>,
) -> anyhow::Result<()> {
    let mut encoded_messages = vec![];

    for message in messages {
        let processor_msg = ProcessorMessage::CosmwasmExecuteMsg { msg: message };

        encoded_messages.push(processor_msg);
    }

    let tx_resp = client
        .execute_wasm(
            authorizations,
            valence_authorization_utils::msg::ExecuteMsg::PermissionlessAction(
                valence_authorization_utils::msg::PermissionlessMsg::SendMsgs {
                    label: label.to_string(),
                    messages: encoded_messages,
                    ttl: None,
                },
            ),
            vec![],
            None,
        )
        .await?;

    debug!("tx hash: {}", tx_resp.hash);

    client.poll_for_tx(&tx_resp.hash).await?;

    Ok(())
}

/// ticks the processor on neutron
pub async fn tick_neutron(client: &NeutronClient, processor: &str) -> anyhow::Result<()> {
    let tx_resp = client
        .execute_wasm(
            processor,
            valence_processor_utils::msg::ExecuteMsg::PermissionlessAction(
                valence_processor_utils::msg::PermissionlessMsg::Tick {},
            ),
            vec![],
            None,
        )
        .await?;

    debug!("tx hash: {}", tx_resp.hash);
    info!(target: DEPOSIT_PHASE, "{}", tx_resp.hash);

    client.poll_for_tx(&tx_resp.hash).await?;

    Ok(())
}

/// constructs the zk authorization execution message and executes it.
/// authorizations module will perform the zk verification and, if
/// successful, push it to the processor for execution
pub async fn post_zkp_on_chain(
    client: &NeutronClient,
    authorizations: &str,
    (proof_program, inputs_program): (Vec<u8>, Vec<u8>),
    proof_domain: Vec<u8>,
) -> anyhow::Result<()> {
    // construct the zk authorization registration message
    let execute_zk_authorization_msg =
        valence_authorization_utils::msg::PermissionlessMsg::ExecuteZkAuthorization {
            label: REGISTER_OBLIGATION_LABEL.to_string(),
            inputs: Binary::from(inputs_program),
            proof: Binary::from(proof_program),
            payload: Binary::from(proof_domain),
        };

    // execute the zk authorization. this will perform the verification
    // and, if successful, push the msg to the processor
    info!(target: REGISTRATION_PHASE, "executing zk authorization");

    let tx_resp = client
        .execute_wasm(
            authorizations,
            valence_authorization_utils::msg::ExecuteMsg::PermissionlessAction(
                execute_zk_authorization_msg,
            ),
            vec![],
            None,
        )
        .await?;

    // poll for inclusion to avoid account sequence mismatch errors
    client.poll_for_tx(&tx_resp.hash).await?;

    Ok(())
}

pub async fn ensure_neutron_account_fees_coverage(
    client: &NeutronClient,
    acc: &str,
) -> anyhow::Result<()> {
    let account_ntrn_balance = client.query_balance(acc, "untrn").await?;

    if account_ntrn_balance < ICA_CONTRACT_FUNDING_AMT {
        let delta = ICA_CONTRACT_FUNDING_AMT - account_ntrn_balance;

        info!(target: DEPOSIT_PHASE, "Funding neutron account with {delta}untrn for ibc tx fees...");
        let transfer_rx = client.transfer(acc, delta, "untrn", None).await?;
        client.poll_for_tx(&transfer_rx.hash).await?;
    }

    Ok(())
}

/// checks that the newly calculated redemption rate is within the acceptable
/// rate update bounds relative to the current rate. pauses the vault otherwise.
pub async fn validate_new_redemption_rate(
    vault: Address,
    client: &EthereumClient,
    eth_rp: &CustomProvider,
    new_redemption_rate: U256,
    max_rate_decrement_bps: u64,
    max_rate_increment_bps: u64,
) -> anyhow::Result<()> {
    let one_way_vault_contract = OneWayVault::new(vault, &eth_rp);

    let current_vault_rate = client
        .query(one_way_vault_contract.redemptionRate())
        .await?
        ._0;

    let current_rate_u128 = u128::try_from(current_vault_rate)?;
    info!(target: UPDATE_PHASE, "pre_update_rate = {current_rate_u128}");

    // get the ratio of newly calculated redemption rate over the previous rate
    let redemption_rate_u128 = u128::try_from(new_redemption_rate)?;
    info!(target: UPDATE_PHASE, "new_rate = {redemption_rate_u128}");

    let rate_change_decimal = Decimal::checked_from_ratio(redemption_rate_u128, current_rate_u128)?;

    info!(target: UPDATE_PHASE, "new to old rate ratio = {rate_change_decimal}");

    match rate_change_decimal.cmp(&Decimal::one()) {
        // rate change is less than 1.0 -> redemption rate decreased
        Ordering::Less => {
            let rate_delta = Decimal::one() - rate_change_decimal;
            info!(target: UPDATE_PHASE, "redemption rate epoch delta = -{rate_delta}");
            let decrement_threshold = Decimal::bps(max_rate_decrement_bps);
            if rate_delta > decrement_threshold {
                warn!(target: UPDATE_PHASE, "rate delta exceeds the threshold of {decrement_threshold}; pausing the vault");
                let pause_request = one_way_vault_contract.pause().into_transaction_request();
                let pause_vault_exec_response = client.sign_and_send(pause_request).await?;
                eth_rp
                    .get_transaction_receipt(pause_vault_exec_response.transaction_hash)
                    .await?;

                return Err(anyhow!(
                    "newly calculated rate exceeds the rate update thresholds"
                ));
            }
        }
        // rate change is exactly 1.0 -> redemption rate did not change
        Ordering::Equal => {
            info!(target: UPDATE_PHASE, "redemption rate epoch delta = 0.0");
        }
        // rate change is greater than 1.0 -> redemption rate increased
        Ordering::Greater => {
            let rate_delta = rate_change_decimal - Decimal::one();
            info!(target: UPDATE_PHASE, "redemption rate epoch delta = +{rate_delta}");
            let increment_threshold = Decimal::bps(max_rate_increment_bps);
            if rate_delta > increment_threshold {
                warn!(target: UPDATE_PHASE, "rate delta exceeds the threshold of {increment_threshold}; pausing the vault");
                let pause_request = one_way_vault_contract.pause().into_transaction_request();
                let pause_vault_exec_response = client.sign_and_send(pause_request).await?;
                eth_rp
                    .get_transaction_receipt(pause_vault_exec_response.transaction_hash)
                    .await?;

                return Err(anyhow!(
                    "newly calculated rate exceeds the rate update thresholds"
                ));
            }
        }
    }

    Ok(())
}
