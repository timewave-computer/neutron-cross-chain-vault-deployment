use std::{env, path::Path};

use anyhow::anyhow;
use lombard_btc_types::{
    ethereum_config::EthereumStrategyConfig, gaia_config::GaiaStrategyConfig,
    lombard_config::LombardStrategyConfig, neutron_config::NeutronStrategyConfig,
};
use packages::{
    ibc_eureka_chain_ids::{EUREKA_COSMOS_HUB_CHAIN_ID, EUREKA_ETHEREUM_CHAIN_ID},
    utils::supervaults::Supervaults,
};
use valence_domain_clients::clients::{
    coprocessor::CoprocessorClient, ethereum::EthereumClient, gaia::CosmosHubClient,
    ibc_eureka_route_client::IBCEurekaRouteClient, lombard::LombardClient, neutron::NeutronClient,
    valence_indexer::OneWayVaultIndexerClient,
};

use serde::{Deserialize, Serialize};
use valence_strategist_utils::worker::ValenceWorkerTomlSerde;

/// top-level config that wraps around each domain configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub ethereum: EthereumStrategyConfig,
    pub neutron: NeutronStrategyConfig,
    pub gaia: GaiaStrategyConfig,
    pub lombard: LombardStrategyConfig,
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
    /// active cosmos hub client
    pub(crate) gaia_client: CosmosHubClient,
    /// active neutron client
    pub(crate) neutron_client: NeutronClient,
    /// active lombard client
    pub(crate) lombard_client: LombardClient,
    /// active one way vault indexer client
    pub(crate) indexer_client: OneWayVaultIndexerClient,
    /// skip route client for IBC eureka
    pub(crate) ibc_eureka_client: IBCEurekaRouteClient,
    /// active coprocessor client
    pub(crate) coprocessor_client: CoprocessorClient,
}

impl Supervaults for Strategy {}

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
        let eureka_api_url = env::var("EUREKA_API_URL")
            .map_err(|e| anyhow!("IBC Eureka route api url must be provided: {e}"))?;
        let strategy_timeout: u64 = env::var("STRATEGY_TIMEOUT")
            .map_err(|e| anyhow!("Strategy timeout must be provided: {e}"))?
            .parse()?;

        let gaia_client = CosmosHubClient::new(
            &cfg.gaia.grpc_url,
            &cfg.gaia.grpc_port,
            &mnemonic,
            &cfg.gaia.chain_id,
            &cfg.gaia.chain_denom,
        )
        .await?;

        let lombard_client = LombardClient::new(
            &cfg.lombard.grpc_url,
            &cfg.lombard.grpc_port,
            &mnemonic,
            &cfg.lombard.chain_id,
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

        let ibc_eureka_client = IBCEurekaRouteClient::new(
            &eureka_api_url,
            EUREKA_ETHEREUM_CHAIN_ID,
            &cfg.ethereum.denoms.deposit_token.to_string(),
            EUREKA_COSMOS_HUB_CHAIN_ID,
            &cfg.gaia.deposit_denom,
        );

        Ok(Self {
            cfg,
            timeout: strategy_timeout,
            eth_client,
            gaia_client,
            neutron_client,
            label,
            indexer_client,
            coprocessor_client,
            ibc_eureka_client,
            lombard_client,
        })
    }

    /// constructor helper that takes in four paths:
    /// - neutron config path
    /// - ethereum config path
    /// - cosmos hub config path
    /// - lombard config path
    ///
    /// reads the configs from those paths, sets up each domain config,
    /// wraps them in a `StrategyConfig`, and uses that to call the initializer above.
    pub async fn from_files<P: AsRef<Path>>(
        neutron_path: P,
        gaia_path: P,
        eth_path: P,
        lombard_path: P,
    ) -> anyhow::Result<Self> {
        let neutron_cfg = NeutronStrategyConfig::from_file(neutron_path)
            .map_err(|e| anyhow!("invalid neutron config: {:?}", e))?;
        let eth_cfg = EthereumStrategyConfig::from_file(eth_path)
            .map_err(|e| anyhow!("invalid ethereum config: {:?}", e))?;
        let gaia_cfg = GaiaStrategyConfig::from_file(gaia_path)
            .map_err(|e| anyhow!("invalid gaia config: {:?}", e))?;
        let lombard_cfg = LombardStrategyConfig::from_file(lombard_path)
            .map_err(|e| anyhow!("invalid lombard config: {:?}", e))?;

        let strategy_cfg = StrategyConfig {
            ethereum: eth_cfg,
            neutron: neutron_cfg,
            gaia: gaia_cfg,
            lombard: lombard_cfg,
        };

        Self::new(strategy_cfg).await
    }
}
