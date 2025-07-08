use crate::strategy_config::Strategy;
use alloy::primitives::Bytes;
use alloy::primitives::U256;
use alloy::providers::Provider;
use alloy::sol_types::SolCall;
use alloy::sol_types::SolValue;
use cosmwasm_std::to_json_binary;
use log::info;
use packages::types::sol_types::AtomicFunction;
use packages::types::sol_types::AtomicSubroutine;
use packages::types::sol_types::Duration;
use packages::types::sol_types::DurationType;
use packages::types::sol_types::Priority;
use packages::types::sol_types::ProcessorMessage;
use packages::types::sol_types::ProcessorMessageType;
use packages::types::sol_types::RetryLogic;
use packages::types::sol_types::RetryTimes;
use packages::types::sol_types::RetryTimesType;
use packages::types::sol_types::SendMsgs;
use packages::types::sol_types::Subroutine;
use packages::types::sol_types::SubroutineType;
use packages::{
    labels::{CCTP_TRANSFER_LABEL, ICA_TRANSFER_LABEL, PROVIDE_LIQUIDIY_LABEL},
    phases::DEPOSIT_PHASE,
    types::sol_types::{
        Authorization, BaseAccount,
        CCTPTransfer::{self},
        ERC20,
    },
    utils::valence_core,
};
use valence_domain_clients::{
    cosmos::base_client::BaseClient,
    evm::base_client::{CustomProvider, EvmBaseClient},
};
use valence_library_utils::OptionUpdate;

impl Strategy {
    pub async fn deposit(&mut self, eth_rp: &CustomProvider) -> anyhow::Result<()> {
        info!(target: DEPOSIT_PHASE, "starting deposit phase");

        // Stage 1: deposit token routing from Ethereum to Noble
        {
            let eth_deposit_token_contract =
                ERC20::new(self.cfg.ethereum.denoms.deposit_token, &eth_rp);
            let eth_deposit_acc = BaseAccount::new(self.cfg.ethereum.accounts.deposit, &eth_rp);

            // query the ethereum deposit account balance
            let eth_deposit_acc_bal = self
                .eth_client
                .query(eth_deposit_token_contract.balanceOf(*eth_deposit_acc.address()))
                .await?
                ._0;
            info!(target: DEPOSIT_PHASE, "eth deposit acc balance = {eth_deposit_acc_bal}");

            // validate that the deposit account balance exceeds the eureka routing
            // threshold amount
            if eth_deposit_acc_bal < self.cfg.ethereum.ibc_transfer_threshold_amt {
                // if balance does not exceed the transfer threshold, we skip the eureka transfer steps
                // and proceed to Noble ica -> Neutron routing
                info!(target: DEPOSIT_PHASE, "CCTP transfer threshold not met! Proceeding to ICA routing.");
            } else {
                // if balance meets the transfer threshold, we carry out the CCTP transfer steps
                // prior to proceeding to Noble ica -> neutron routing
                info!(target: DEPOSIT_PHASE, "CCTP transfer threshold met!");

                self.eth_to_noble_routing(eth_rp, eth_deposit_acc_bal)
                    .await?;
            }
        }

        // Stage 2: deposit token routing from Noble to Neutron
        {
            let noble_ica_bal = self
                .noble_client
                .query_balance(
                    &self.cfg.noble.forwarding_account,
                    &self.cfg.noble.chain_denom,
                )
                .await?;
            info!(target: DEPOSIT_PHASE, "Noble ICA balance = {noble_ica_bal}");

            // depending on the Noble ICA deposit token balance, we either perform the ICA IBC routing
            // of the balances to Neutron, or proceed to the next stage
            if noble_ica_bal == 0 {
                info!(target: DEPOSIT_PHASE, "nothing to bridge; proceeding to position entry");
            } else {
                info!(target: DEPOSIT_PHASE, "pulling funds to Neutron...");
                self.noble_to_neutron_routing(noble_ica_bal).await?;
            }
        }

        // Stage 3: Supervault position entry on Neutron
        {
            let neutron_deposit_bal = self
                .neutron_client
                .query_balance(
                    &self.cfg.neutron.accounts.deposit,
                    &self.cfg.neutron.denoms.deposit_token,
                )
                .await?;
            info!(target: DEPOSIT_PHASE, "Neutron deposit account balance = {neutron_deposit_bal}");

            // depending on the neutron deposit account balance, we either conclude the deposit phase
            // or perform the Supervault liquidity provision with the available balance.
            if neutron_deposit_bal == 0 {
                info!(target: DEPOSIT_PHASE, "Neutron deposit account balance is insufficient for entry! concluding the deposit phase...");
            } else {
                info!(target: DEPOSIT_PHASE, "Neutron deposit account balance = {neutron_deposit_bal}; LPing...");
                self.enter_supervaults_position().await?;
            }
        }

        Ok(())
    }

    /// carries out the steps needed to route the deposits from Ethereum program deposit
    /// account to the configured Noble ICA managed by Neutron Valence-ICA.
    async fn eth_to_noble_routing(
        &mut self,
        eth_rp: &CustomProvider,
        eth_deposit_acc_bal: U256,
    ) -> anyhow::Result<()> {
        let eth_auth_contract = Authorization::new(self.cfg.ethereum.authorizations, &eth_rp);
        let eth_deposit_acc_bal_u128 = u128::try_from(eth_deposit_acc_bal)?;

        // transfer can be considered complete when the current ica balance increases
        // by the amount available on eth deposit account (TODO: minus fees?)
        let pre_routing_noble_ica_bal = self
            .noble_client
            .query_balance(
                &self.cfg.noble.forwarding_account,
                &self.cfg.noble.chain_denom,
            )
            .await?;

        let transfer_call = CCTPTransfer::transferCall {};
        let encoded_transfer_call = transfer_call.abi_encode();
        let atomic_function = AtomicFunction {
            contractAddress: self.cfg.ethereum.libraries.cctp_transfer,
        };

        // Create retry logic with NoRetry for atomic execution
        let retry_logic = RetryLogic {
            times: RetryTimes {
                retryType: RetryTimesType::NoRetry,
                amount: 0,
            },
            interval: Duration {
                durationType: DurationType::Time,
                value: 0,
            },
        };

        // Create AtomicSubroutine
        let atomic_subroutine = AtomicSubroutine {
            functions: vec![atomic_function],
            retryLogic: retry_logic,
        };

        // Encode the atomic subroutine
        let encoded_subroutine = atomic_subroutine.abi_encode();

        // Create Subroutine wrapper
        let subroutine = Subroutine {
            subroutineType: SubroutineType::Atomic,
            subroutine: Bytes::from(encoded_subroutine),
        };

        // Create SendMsgs message with the properly encoded transfer call
        let send_msgs = SendMsgs {
            executionId: 1, // Generated execution ID
            priority: Priority::Medium,
            subroutine,
            expirationTime: 0, // No expiration
            messages: vec![Bytes::from(encoded_transfer_call)],
        };

        // Encode SendMsgs
        let encoded_send_msgs = send_msgs.abi_encode();

        // Create ProcessorMessage
        let processor_message = ProcessorMessage {
            messageType: ProcessorMessageType::SendMsgs,
            message: Bytes::from(encoded_send_msgs),
        };

        let enqueue_msg_tx_request = eth_auth_contract
            .sendProcessorMessage(
                CCTP_TRANSFER_LABEL.to_string(),
                Bytes::from(processor_message.abi_encode()),
            )
            .into_transaction_request();

        let enqueue_cctp_exec_response = self
            .eth_client
            .sign_and_send(enqueue_msg_tx_request)
            .await?;

        eth_rp
            .get_transaction_receipt(enqueue_cctp_exec_response.transaction_hash)
            .await?;

        let noble_ica_expected_balance = pre_routing_noble_ica_bal + eth_deposit_acc_bal_u128;
        info!(
            target: DEPOSIT_PHASE,
            "Noble ica expected bal = {noble_ica_expected_balance}; polling..."
        );

        // block execution until the funds arrive to the Noble ICA owned
        // by the Valence Interchain Account on Neutron.
        // poll for 15sec * 100 = 1500sec = 25min.
        self.noble_client
            .poll_until_expected_balance(
                &self.cfg.noble.forwarding_account,
                &self.cfg.noble.chain_denom,
                noble_ica_expected_balance,
                15,  // every 15 sec
                100, // for 100 times
            )
            .await?;
        Ok(())
    }

    async fn noble_to_neutron_routing(&mut self, noble_ica_bal: u128) -> anyhow::Result<()> {
        let ica_ibc_transfer_update_msg: valence_library_utils::msg::ExecuteMsg<
            valence_ica_ibc_transfer::msg::FunctionMsgs,
            valence_ica_ibc_transfer::msg::LibraryConfigUpdate,
        > = valence_library_utils::msg::ExecuteMsg::UpdateConfig {
            new_config: valence_ica_ibc_transfer::msg::LibraryConfigUpdate {
                input_addr: None,
                amount: Some(noble_ica_bal.into()),
                denom: None,
                receiver: None,
                memo: None,
                remote_chain_info: None,
                denom_to_pfm_map: None,
                eureka_config: OptionUpdate::Set(None),
            },
        };
        let ica_ibc_transfer_exec_msg =
            valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
                valence_ica_ibc_transfer::msg::FunctionMsgs::Transfer {},
            );

        info!(target: DEPOSIT_PHASE, "enqueuing ica_ibc_transfer library update & transfer");
        valence_core::enqueue_neutron(
            &self.neutron_client,
            &self.cfg.neutron.authorizations,
            ICA_TRANSFER_LABEL,
            vec![
                to_json_binary(&ica_ibc_transfer_update_msg)?,
                to_json_binary(&ica_ibc_transfer_exec_msg)?,
            ],
        )
        .await?;

        valence_core::ensure_neutron_account_fees_coverage(
            &self.neutron_client,
            &self.cfg.neutron.accounts.deposit,
        )
        .await?;

        // transfer can be considered complete when the current ica balance increases
        // by the expected post_fee ibc eureka transfer amount out
        let pre_routing_neutron_deposit_acc_bal = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.deposit,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;

        let neutron_deposit_acc_expected_bal = pre_routing_neutron_deposit_acc_bal + noble_ica_bal;
        info!(
            target: DEPOSIT_PHASE,
            "neutron deposit acc expected bal = {neutron_deposit_acc_expected_bal}; polling..."
        );

        info!(target: DEPOSIT_PHASE, "tick: update & transfer");
        valence_core::tick_neutron(&self.neutron_client, &self.cfg.neutron.processor).await?;

        info!(target: DEPOSIT_PHASE, "polling for neutron deposit account to receive the funds");

        // block execution until funds arrive to the Neutron program deposit
        // account
        self.neutron_client
            .poll_until_expected_balance(
                &self.cfg.neutron.accounts.deposit,
                &self.cfg.neutron.denoms.deposit_token,
                neutron_deposit_acc_expected_bal,
                5,
                30,
            )
            .await?;

        Ok(())
    }

    async fn enter_supervaults_position(&mut self) -> anyhow::Result<()> {
        // use Supervaults lper library to deposit funds from Supervaults deposit account
        // into the configured supervault
        let supervaults_lper_execute_msg =
            valence_library_utils::msg::ExecuteMsg::<_, ()>::ProcessFunction(
                valence_supervaults_lper::msg::FunctionMsgs::ProvideLiquidity {
                    expected_vault_ratio_range: None,
                },
            );

        // enqueue all three actions under a single label as its an atomic subroutine
        valence_core::enqueue_neutron(
            &self.neutron_client,
            &self.cfg.neutron.authorizations,
            PROVIDE_LIQUIDIY_LABEL,
            vec![to_json_binary(&supervaults_lper_execute_msg)?],
        )
        .await?;

        valence_core::tick_neutron(&self.neutron_client, &self.cfg.neutron.processor).await?;

        Ok(())
    }
}
