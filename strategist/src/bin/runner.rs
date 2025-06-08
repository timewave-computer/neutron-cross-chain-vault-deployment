use dotenv::dotenv;
use std::{env, error::Error};
use valence_strategist_utils::worker::ValenceWorker;

use strategist::strategy_config::Strategy;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // load environment variables
    dotenv().ok();

    // get configuration paths from environment variables
    let neutron_cfg_path = env::var("NEUTRON_CFG_PATH")?;
    let ethereum_cfg_path = env::var("ETHEREUM_CFG_PATH")?;
    let gaia_cfg_path = env::var("GAIA_CFG_PATH")?;

    println!("Using configuration files:");
    println!("  Neutron: {}", neutron_cfg_path);
    println!("  Ethereum: {}", ethereum_cfg_path);
    println!("  Gaia: {}", gaia_cfg_path);

    // initialize the strategy from configuration files
    let strategy =
        Strategy::from_files(&neutron_cfg_path, &gaia_cfg_path, &ethereum_cfg_path).await?;

    // start the strategy and get the thread join handle
    let _strategist_join_handle = strategy.start();

    Ok(())
}
