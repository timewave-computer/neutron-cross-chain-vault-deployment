use std::{env, path::Path};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use usdc_types::{
    ethereum_config::EthereumStrategyConfig, neutron_config::NeutronStrategyConfig,
    noble_config::NobleStrategyConfig,
};
use valence_domain_clients::clients::{
    coprocessor::CoprocessorClient, ethereum::EthereumClient, neutron::NeutronClient,
    noble::NobleClient, valence_indexer::OneWayVaultIndexerClient,
};
use valence_strategist_utils::worker::ValenceWorkerTomlSerde;

/// top-level config that wraps around each domain configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub ethereum: EthereumStrategyConfig,
    pub neutron: NeutronStrategyConfig,
    pub noble: NobleStrategyConfig,
}

// main strategy struct that wraps around the StrategyConfig
// and stores the initialized clients
pub struct Strategy {
    /// strategy name
    pub label: String,

    /// strategy timeout (in seconds)
    pub timeout: u64,

    /// top level strategy configuration
    pub cfg: StrategyConfig,

    /// active ethereum client
    pub(crate) eth_client: EthereumClient,
    /// active neutron client
    pub(crate) neutron_client: NeutronClient,
    /// active noble client
    pub(crate) noble_client: NobleClient,
    /// active one way vault indexer client
    pub(crate) indexer_client: OneWayVaultIndexerClient,
    /// active coprocessor client
    pub(crate) coprocessor_client: CoprocessorClient,
}

#[allow(dead_code)]
impl Strategy {
    /// strategy initializer that takes in a `StrategyConfig`, and uses it
    /// to initialize the respective domain clients. prerequisite to starting
    /// the strategist.
    pub async fn new(cfg: StrategyConfig) -> anyhow::Result<Self> {
        let mnemonic =
            env::var("MNEMONIC").map_err(|e| anyhow!("mnemonic must be provided: {e}"))?;
        let label = env::var("LABEL").map_err(|e| anyhow!("label must be provided: {e}"))?;
        let indexer_api_key = env::var("INDEXER_API_KEY")
            .map_err(|e| anyhow!("indexer api key must be provided: {e}"))?;
        let indexer_api_url = env::var("INDEXER_API_URL")
            .map_err(|e| anyhow!("indexer api url key must be provided: {e}"))?;
        let strategy_timeout: u64 = env::var("STRATEGY_TIMEOUT")
            .map_err(|e| anyhow!("Strategy timeout must be provided: {e}"))?
            .parse()?;

        let noble_client = NobleClient::new(
            &cfg.noble.grpc_url,
            &cfg.noble.grpc_port,
            &mnemonic,
            &cfg.noble.chain_id,
            "ustake",
        )
        .await?;

        let neutron_client = NeutronClient::new(
            &cfg.neutron.grpc_url,
            &cfg.neutron.grpc_port,
            &mnemonic,
            &cfg.neutron.chain_id,
        )
        .await?;

        let eth_client = EthereumClient::new(&cfg.ethereum.rpc_url, &mnemonic, None)?;

        let indexer_client = OneWayVaultIndexerClient::new(
            &indexer_api_url,
            &indexer_api_key,
            &cfg.ethereum.libraries.one_way_vault.to_string(),
        );

        let coprocessor_client = CoprocessorClient::default();

        Ok(Self {
            cfg,
            timeout: strategy_timeout,
            eth_client,
            neutron_client,
            label,
            indexer_client,
            coprocessor_client,
            noble_client,
        })
    }

    /// constructor helper that takes in four paths:
    /// - neutron config path
    /// - ethereum config path
    /// - noble config path
    ///
    /// reads the configs from those paths, sets up each domain config,
    /// wraps them in a `StrategyConfig`, and uses that to call the initializer above.
    pub async fn from_files<P: AsRef<Path>>(
        neutron_path: P,
        eth_path: P,
        noble_path: P,
    ) -> anyhow::Result<Self> {
        let neutron_cfg = NeutronStrategyConfig::from_file(neutron_path)
            .map_err(|e| anyhow!("invalid neutron config: {:?}", e))?;
        let eth_cfg = EthereumStrategyConfig::from_file(eth_path)
            .map_err(|e| anyhow!("invalid ethereum config: {:?}", e))?;
        let noble_cfg = NobleStrategyConfig::from_file(noble_path)
            .map_err(|e| anyhow!("invalid noble config: {:?}", e))?;

        let strategy_cfg = StrategyConfig {
            ethereum: eth_cfg,
            neutron: neutron_cfg,
            noble: noble_cfg,
        };

        Self::new(strategy_cfg).await
    }
}
