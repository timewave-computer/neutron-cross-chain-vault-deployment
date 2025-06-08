use std::error::Error;
use dotenv::dotenv;
use valence_strategist_utils::worker::ValenceWorker;

use strategist::strategy_config::Strategy;

// default configuration paths, should be overridden by environment variables
const NEUTRON_CFG_PATH: &str = "config/neutron_config.toml";
const ETHEREUM_CFG_PATH: &str = "config/ethereum_config.toml";
const GAIA_CFG_PATH: &str = "config/gaia_config.toml";


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Load environment variables
    dotenv().ok();

    // initialize the strategy from configuration files
    let strategy = Strategy::from_files(NEUTRON_CFG_PATH, GAIA_CFG_PATH, ETHEREUM_CFG_PATH).await?;

    let _strategist_join_handle = strategy.start();

    Ok(())
}
