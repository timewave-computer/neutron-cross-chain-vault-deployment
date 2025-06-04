use std::{error::Error, str::FromStr};

use alloy::{
    primitives::{Address, B256, Log, keccak256},
    providers::Provider,
    sol_types::SolEvent,
};
use async_trait::async_trait;
use cosmwasm_std::to_json_binary;
use log::{info, warn};
use serde::Serialize;
use types::sol_types::{BaseAccount, ERC20, OneWayVault::WithdrawRequested};
use valence_authorization_utils::msg::ProcessorMessage;
use valence_clearing_queue::msg::ObligationsResponse;
use valence_domain_clients::{
    cosmos::{base_client::BaseClient, wasm_client::WasmClient},
    evm::{
        base_client::{CustomProvider, EvmBaseClient},
        request_provider_client::RequestProviderClient,
    },
};
use valence_strategist_utils::worker::ValenceWorker;

use crate::strategy_config::Strategy;

// implement the ValenceWorker trait for the Strategy struct.
// This trait defines the main loop of the strategy and inherits
// the default implementation for spawning the worker.
#[async_trait]
impl ValenceWorker for Strategy {
    fn get_name(&self) -> String {
        format!("Valence X-Vault: {}", self.label)
    }

    async fn cycle(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!("{}: Starting cycle...", self.get_name());

        let eth_rp: CustomProvider = self.eth_client.get_request_provider().await?;

        // first we carry out the deposit flow
        self.deposit(&eth_rp).await?;

        // after deposit flow is complete, we process the new obligations
        self.register_withdraw_obligations(&eth_rp).await?;

        // with new obligations registered into the clearing queue, we
        // carry out the settlements
        self.settlement().await?;

        // after the deposit flow and having processed all new exit
        // requests, the epoch is ready to be concluded. we perform the
        // accounting flow and post vault update.
        self.update().await?;

        Ok(())
    }
}

impl Strategy {
    /// enqueues a message on neutron
    async fn enqueue_neutron<T>(
        &mut self,
        label: &str,
        messages: Vec<T>,
    ) -> Result<(), Box<dyn Error + Send + Sync>>
    where
        T: Serialize,
    {
        let mut encoded_messages = vec![];

        for message in messages {
            let encoded_msg = to_json_binary(&message)?;

            let processor_msg = ProcessorMessage::CosmwasmExecuteMsg { msg: encoded_msg };

            encoded_messages.push(processor_msg);
        }

        self.neutron_client
            .execute_wasm(
                &self.cfg.neutron.authorizations,
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

        Ok(())
    }

    /// ticks the processor on neutron
    async fn tick_neutron(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.neutron_client
            .execute_wasm(
                &self.cfg.neutron.processor,
                valence_processor_utils::msg::ExecuteMsg::PermissionlessAction(
                    valence_processor_utils::msg::PermissionlessMsg::Tick {},
                ),
                vec![],
                None,
            )
            .await?;

        Ok(())
    }

    /// performs the vault rate update
    async fn update(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }

    /// carries out the steps needed to bring the new deposits from Ethereum to
    /// Neutron (via Cosmos Hub) before depositing them into Mars protocol.
    async fn deposit(
        &mut self,
        eth_rp: &CustomProvider,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let eth_wbtc_contract = ERC20::new(
            // TODO: make ethereum cfg return `Address` instead of string
            // for eth addresses
            Address::from_str(&self.cfg.ethereum.denoms.deposit_token)?,
            &eth_rp,
        );
        let eth_deposit_acc = BaseAccount::new(
            Address::from_str(&self.cfg.ethereum.accounts.deposit)?,
            &eth_rp,
        );

        // 1. query the ethereum deposit account balance
        let eth_deposit_acc_bal = self
            .eth_client
            .query(eth_wbtc_contract.balanceOf(*eth_deposit_acc.address()))
            .await?
            ._0;

        // 2. validate that the deposit account balance exceeds the eureka routing
        // threshold amount
        if eth_deposit_acc_bal < self.cfg.ethereum.ibc_transfer_threshold_amt {
            // early return if balance is too small for the eureka transfer
            // to be worth it
            return Ok(());
        }

        // 3. perform IBC-Eureka transfer to Cosmos Hub ICA

        // 4. block execution until the funds arrive to the Cosmos Hub ICA owned
        // by the Valence Interchain Account on Neutron
        // TODO: make this into a blocking assertion query
        self.gaia_client
            .query_balance("TODO:GAIA_ICA", &self.cfg.gaia.btc_denom)
            .await?;

        self.enqueue_neutron("ICA_IBC_UPDATE_AMOUNT", vec!["TODO"])
            .await?;

        self.tick_neutron().await?;

        // 5. Initiate ICA-IBC-Transfer from Cosmos Hub ICA to Neutron program
        // deposit account
        self.enqueue_neutron(
            "ICA_IBC_TRANSFER",
            vec![valence_ica_ibc_transfer::msg::FunctionMsgs::Transfer {}],
        )
        .await?;

        self.tick_neutron().await?;

        // 6. block execution until funds arrive to the Neutron program deposit
        // account
        // TODO: make this into a blocking assertion query
        self.neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.deposit,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;

        // 7. use Valence Forwarder to route funds from the Neutron program
        // deposit account to the Mars deposit account
        self.enqueue_neutron(
            "DEPOSIT_FWD",
            vec![valence_forwarder_library::msg::FunctionMsgs::Forward {}],
        )
        .await?;

        self.tick_neutron().await?;

        // 8. use Mars Lending library to deposit funds from Mars deposit account
        // into Mars protocol
        self.enqueue_neutron(
            "MARS_DEPOSIT",
            vec![valence_mars_lending::msg::FunctionMsgs::Lend {}],
        )
        .await?;

        self.tick_neutron().await?;

        Ok(())
    }

    /// reads the newly submitted withdrawal obligations that are not yet
    /// present in the Clearing Queue, generates their zero-knowledge proofs,
    /// and posts them into the Clearing queue in order.
    async fn register_withdraw_obligations(
        &mut self,
        eth_rp: &CustomProvider,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // 1. query the Clearing Queue library for the latest posted withdraw request ID
        let _clearing_queue_cfg: valence_clearing_queue::msg::Config = self
            .neutron_client
            .query_contract_state(
                &self.cfg.neutron.libraries.clearing_queue,
                valence_clearing_queue::msg::QueryMsg::GetLibraryConfig {},
            )
            .await?;

        // 2. query the OneWayVault for emitted events and filter them such that
        // only requests with id greater than the one queried in step 1. are fetched
        let vault_addr = self
            .cfg
            .ethereum
            .libraries
            .one_way_vault
            .parse::<Address>()?;
        let event_signature = "WithdrawRequested(uint64,address,string,uint256)";
        let event_signature_hash = keccak256(event_signature.as_bytes());
        let event_topic = B256::from(event_signature_hash);

        // TODO: can we tune this filter such that only events with id (uint64 in signature)
        // are fetched? ideally by mapping a _clearing_queue_cfg.latest_id to the eth
        // block on which that withdraw request was submitted to the vault and setting
        // that with .from_block()
        let withdraw_event_filter = alloy::rpc::types::Filter::new()
            .address(vault_addr)
            .event_signature(event_topic);

        let logs = eth_rp.get_logs(&withdraw_event_filter).await?;

        // todo: probably better use btreemap to have the events sorted
        // by id on insertion below
        let mut withdraw_requested_events = vec![];

        for log in logs {
            let alloy_log = Log::new(log.address(), log.topics().into(), log.data().clone().data)
                .unwrap_or_default();

            match WithdrawRequested::decode_log(&alloy_log, false) {
                Ok(val) => {
                    info!("[BTC_STRATEGIST] decoded WithdrawRequested log: {:?}", val);
                    withdraw_requested_events.push(val);
                }
                Err(e) => warn!(
                    "[BTC_STRATEGIST] failed to decode WithdrawRequested log: {:?}",
                    e
                ),
            }
        }

        // 3. process the new OneWayVault Withdraw events in order from the oldest
        // to the newest, posting them to the coprocessor to obtain a ZKP

        for _withdraw_request in withdraw_requested_events {
            // TODO: post to coprocessor, get ZKP

            //  4. preserving the order, post the ZKPs obtained in step 3. to the Neutron
            // Authorizations contract, enqueuing them to the processor
            self.enqueue_neutron("POST_ZKP", vec!["TODO"]).await?;

            // 5. tick the processor to register the obligations to the clearing queue
            self.tick_neutron().await?;
        }

        Ok(())
    }

    /// performs the final settlement of registered withdrawal obligations in
    /// the Clearing Queue library. this involves topping up the settlement
    /// account with funds necessary to carry out all withdrawal obligations
    /// in the queue.
    async fn settlement(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // 1. query the current settlement account balance
        let settlement_acc_bal = self
            .neutron_client
            .query_balance(
                &self.cfg.neutron.accounts.settlement,
                &self.cfg.neutron.denoms.deposit_token,
            )
            .await?;

        // 2. query the Clearing Queue and calculate the total active obligations
        let clearing_queue: ObligationsResponse = self
            .neutron_client
            .query_contract_state(
                &self.cfg.neutron.libraries.clearing_queue,
                valence_clearing_queue::msg::QueryMsg::PendingObligations {
                    from: None,
                    to: None,
                },
            )
            .await?;

        // sum the total obligations amount
        let total_queue_obligations: u128 = clearing_queue
            .obligations
            .iter()
            .map(|o| o.payout_coin.amount.u128())
            .sum();

        // 3. if settlement account balance is insufficient to cover the active
        // obligations, we perform the Mars protocol withdrawals
        if settlement_acc_bal < total_queue_obligations {
            // 3. simulate Mars protocol withdrawal to obtain the funds necessary
            // to fulfill all active withdrawal requests
            let obligations_delta = total_queue_obligations - settlement_acc_bal;

            // 4. call the Mars lending library to perform the withdrawal.
            // This will deposit the underlying assets directly to the settlement account.
            self.enqueue_neutron(
                "MARS_WITHDRAW",
                vec![&valence_mars_lending::msg::FunctionMsgs::Withdraw {
                    amount: Some(obligations_delta.into()),
                }],
            )
            .await?;

            self.tick_neutron().await?;
        }

        // 5. process the Clearing Queue settlement requests by enqueuing the settlement
        // messages to the processor and ticking
        for _ in clearing_queue.obligations {
            self.enqueue_neutron(
                "CLEAR_SETTLEMENTS",
                vec![valence_clearing_queue::msg::FunctionMsgs::SettleNextObligation {}],
            )
            .await?;

            self.tick_neutron().await?;
        }

        Ok(())
    }
}
