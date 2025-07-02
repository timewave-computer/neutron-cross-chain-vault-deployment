use log::{info, warn};
use lombard_btc_strategist::strategy_config::Strategy;
use std::env;
use valence_strategist_utils::worker::ValenceWorker;
use packages::utils::logging::setup_logging;

const RUNNER: &str = "runner";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // load environment variables
    let env_path = env::current_dir()?.join("strategies/lombard_btc/strategist/lbtc.env");
    dotenv::from_path(env_path.as_path())?;

    setup_logging().await?;

    info!(target: RUNNER, "starting the strategist runner");

    // get configuration paths from environment variables
    let neutron_cfg_path = env::var("NEUTRON_CFG_PATH")
        .map_err(|e| anyhow::Error::msg(format!("neutron cfg path not found: {e}")))?;
    let ethereum_cfg_path = env::var("ETHEREUM_CFG_PATH")
        .map_err(|e| anyhow::Error::msg(format!("eth cfg path not found: {e}")))?;
    let gaia_cfg_path = env::var("GAIA_CFG_PATH")
        .map_err(|e| anyhow::Error::msg(format!("gaia cfg path not found: {e}")))?;
    let lombard_cfg_path = env::var("LOMBARD_CFG_PATH")
        .map_err(|e| anyhow::Error::msg(format!("lombard cfg path not found: {e}")))?;
    info!(target: RUNNER, "Using configuration files:");
    info!(target: RUNNER, "  Neutron: {}", neutron_cfg_path);
    info!(target: RUNNER, "  Ethereum: {}", ethereum_cfg_path);
    info!(target: RUNNER, "  Gaia: {}", gaia_cfg_path);
    info!(target: RUNNER, "  Lombard: {}", lombard_cfg_path);

    // initialize the strategy from configuration files
    let strategy = Strategy::from_files(
        &neutron_cfg_path,
        &gaia_cfg_path,
        &ethereum_cfg_path,
        &lombard_cfg_path,
    )
    .await?;

    info!(target: RUNNER, "strategy initialized");
    info!(target: RUNNER, "starting the strategist");

    // start the strategy and get the thread join handle
    let strategist_join_handle = strategy.start();

    // join here will wait for the strategist thread to finish which should never happen in practice since it runs an infinite stayalive loop
    match strategist_join_handle.join() {
        Ok(t) => warn!(target: RUNNER, "strategist thread completed: {:?}", t),
        Err(e) => warn!(target: RUNNER, "strategist thread completed with error: {:?}", e),
    }

    Ok(())
}
