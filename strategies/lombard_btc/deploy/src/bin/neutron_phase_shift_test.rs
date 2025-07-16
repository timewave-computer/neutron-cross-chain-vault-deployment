use std::{env, error::Error, fs};

use cosmwasm_std::{Decimal, to_json_binary};
use lombard_btc_deploy::OUTPUTS_DIR;
use lombard_btc_types::neutron_config::NeutronStrategyConfig;
use packages::labels::PHASE_SHIFT_LABEL;
use valence_authorization_utils::msg::ProcessorMessage;
use valence_domain_clients::{
    clients::neutron::NeutronClient,
    cosmos::{base_client::BaseClient, wasm_client::WasmClient},
};
use valence_library_utils::{OptionUpdate, liquidity_utils::AssetData};
use valence_supervaults_lper::msg::LiquidityProviderConfig;

const MAXBTC_ISSUER: &str = "neutron1qh7fj57et5ac645k272pa6w089tlhj678a3dtr7k0lp523p37q2sase0td";
const NEW_SUPERVAULT_ADDRESS: &str =
    "neutron1ct00dzd944axt0pw9ak25a3ar40n8tmdwwnqxv6dpxxy6v3ysz9sw7n3jd";
const NEW_ASSET1_DENOM: &str =
    "ibc/0E293A7622DC9A6439DB60E6D234B5AF446962E27CA3AB44D0590603DFF6968E";
const NEW_ASSET2_DENOM: &str =
    "ibc/2EB30350120BBAFC168F55D0E65551A27A724175E8FBCC7B37F9A71618FE136B";
const NEW_LP_DENOM: &str =
    "factory/neutron1ct00dzd944axt0pw9ak25a3ar40n8tmdwwnqxv6dpxxy6v3ysz9sw7n3jd/BTC-BTC";
const SUPERVAULT_SENDER: &str =
    "neutron1kfdtk5da6phzynyulacp7z49nuu30njpz5wh5th7gxugmn4d5r2s9rqhjj";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv::dotenv().ok();
    let mnemonic = env::var("MNEMONIC").expect("mnemonic must be provided");

    let current_dir = env::current_dir()?;

    let ntrn_cfg =
        fs::read_to_string(current_dir.join(format!("{OUTPUTS_DIR}/neutron_strategy_config.toml")))
            .expect("Failed to read file");

    let ntrn_strategy_config: NeutronStrategyConfig =
        toml::from_str(&ntrn_cfg).expect("Failed to parse Neutron strategy config");

    let authorization_contract = ntrn_strategy_config.authorizations;

    let neutron_client = NeutronClient::new(
        &ntrn_strategy_config.grpc_url,
        &ntrn_strategy_config.grpc_port,
        &mnemonic,
        &ntrn_strategy_config.chain_id,
    )
    .await?;

    // Prepare message for phase shift
    let withdraw_liquidity_msg = valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
        valence_supervaults_withdrawer::msg::FunctionMsgs::WithdrawLiquidity {
            expected_vault_ratio_range: None,
        },
    );
    let withdraw_liquidity_msg = to_json_binary(&withdraw_liquidity_msg)?;

    let update_maxbtc_issuer_config: valence_library_utils::msg::ExecuteMsg<
        valence_maxbtc_issuer::msg::FunctionMsgs,
        valence_maxbtc_issuer::msg::LibraryConfigUpdate,
    > = valence_library_utils::msg::ExecuteMsg::UpdateConfig {
        new_config: valence_maxbtc_issuer::msg::LibraryConfigUpdate {
            input_addr: None,
            output_addr: None,
            maxbtc_issuer_addr: Some(MAXBTC_ISSUER.to_string()),
            btc_denom: None,
        },
    };
    let update_maxbtc_issuer_config = to_json_binary(&update_maxbtc_issuer_config)?;
    let issue_maxbtc_msg = valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
        valence_maxbtc_issuer::msg::FunctionMsgs::Issue {},
    );
    let issue_maxbtc_msg = to_json_binary(&issue_maxbtc_msg)?;
    let forward_to_supervault_deposit_account_msg =
        valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
            valence_forwarder_library::msg::FunctionMsgs::Forward {},
        );
    let forward_to_supervault_deposit_account_msg =
        to_json_binary(&forward_to_supervault_deposit_account_msg)?;
    let update_supervault_lper_config: valence_library_utils::msg::ExecuteMsg<
        valence_supervaults_lper::msg::FunctionMsgs,
        valence_supervaults_lper::msg::LibraryConfigUpdate,
    > = valence_library_utils::msg::ExecuteMsg::UpdateConfig {
        new_config: valence_supervaults_lper::msg::LibraryConfigUpdate {
            input_addr: None,
            output_addr: None,
            vault_addr: Some(NEW_SUPERVAULT_ADDRESS.to_string()),
            lp_config: Some(LiquidityProviderConfig {
                asset_data: AssetData {
                    asset1: NEW_ASSET1_DENOM.to_string(),
                    asset2: NEW_ASSET2_DENOM.to_string(),
                },
                lp_denom: NEW_LP_DENOM.to_string(),
            }),
        },
    };
    let update_supervault_lper_config = to_json_binary(&update_supervault_lper_config)?;
    let update_clearing_queue_config: valence_library_utils::msg::ExecuteMsg<
        valence_clearing_queue_supervaults::msg::FunctionMsgs,
        valence_clearing_queue_supervaults::msg::LibraryConfigUpdate,
    > = valence_library_utils::msg::ExecuteMsg::UpdateConfig {
        new_config: valence_clearing_queue_supervaults::msg::LibraryConfigUpdate {
            settlement_acc_addr: None,
            denom: None,
            latest_id: OptionUpdate::None,
            mars_settlement_ratio: None,
            supervaults_settlement_info: Some(vec![
                valence_clearing_queue_supervaults::msg::SupervaultSettlementInfo {
                    supervault_addr: NEW_SUPERVAULT_ADDRESS.to_string(),
                    supervault_sender: SUPERVAULT_SENDER.to_string(),
                    settlement_ratio: Decimal::one(),
                },
            ]),
        },
    };
    let update_clearing_queue_config = to_json_binary(&update_clearing_queue_config)?;

    let mut encoded_messages = vec![];
    for message in [
        withdraw_liquidity_msg,
        update_maxbtc_issuer_config,
        issue_maxbtc_msg,
        forward_to_supervault_deposit_account_msg,
        update_supervault_lper_config,
        update_clearing_queue_config,
    ] {
        let processor_msg = ProcessorMessage::CosmwasmExecuteMsg { msg: message };
        encoded_messages.push(processor_msg);
    }

    let authorization_msg = valence_authorization_utils::msg::ExecuteMsg::PermissionlessAction(
        valence_authorization_utils::msg::PermissionlessMsg::SendMsgs {
            label: PHASE_SHIFT_LABEL.to_string(),
            messages: encoded_messages.clone(),
            ttl: None,
        },
    );

    println!("Authorization msg: {authorization_msg:?}");

    /*let tx_resp = neutron_client
        .execute_wasm(
            &authorization_contract,
            authorization_msg,
            vec![],
            None,
        )
        .await?;
    neutron_client.poll_for_tx(&tx_resp.hash).await?;

    let tx_resp = neutron_client
        .execute_wasm(
            &ntrn_strategy_config.processor,
            valence_processor_utils::msg::ExecuteMsg::PermissionlessAction(
                valence_processor_utils::msg::PermissionlessMsg::Tick {},
            ),
            vec![],
            None,
        )
        .await?;
    neutron_client.poll_for_tx(&tx_resp.hash).await?;*/

    Ok(())
}
