use dotenv::dotenv;
use log::{info, warn};
use std::{env, error::Error};
use valence_strategist_utils::worker::ValenceWorker;
use wbtc_test_strategist::strategy_config::Strategy;
use opentelemetry_otlp::{WithExportConfig, Protocol};
use opentelemetry_appender_log::{OpenTelemetryLogBridge};
use opentelemetry_sdk::logs::{BatchLogProcessor, SdkLoggerProvider};
use opentelemetry_sdk::{Resource};
use multi_log;

const RUNNER: &str = "runner";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // load environment variables
    dotenv().ok();

    match env::var("OTLP_ENDPOINT") {
        Ok(otlp_endpoint) => {
            let otlp_exporter = opentelemetry_otlp::LogExporter::builder()
                .with_http()
                .with_protocol(Protocol::HttpBinary)
                .with_endpoint(otlp_endpoint)
                .build()?;
            let otlp_logger_provider = SdkLoggerProvider::builder()
                .with_resource(
                    Resource::builder()
                    .with_service_name("neutron-strategist")
                    .build(),
                )
                .with_log_processor(BatchLogProcessor::builder(otlp_exporter).build())
                .build();
            let otlp_logger = Box::new(OpenTelemetryLogBridge::new(&otlp_logger_provider));
            let std_logger = Box::new(env_logger::Builder::from_default_env().build());
            let _ = multi_log::MultiLogger::init(vec![otlp_logger, std_logger], log::Level::Trace);

        },
        Err(_) => {
            env_logger::init();
            info!(target: RUNNER, "OTLP_ENDPOINT not set, skipping OpenTelemetry logging");
        }
    };
    
    info!(target: RUNNER, "starting the strategist runner");

    // get configuration paths from environment variables
    let neutron_cfg_path = env::var("NEUTRON_CFG_PATH")?;
    let ethereum_cfg_path = env::var("ETHEREUM_CFG_PATH")?;
    let gaia_cfg_path = env::var("GAIA_CFG_PATH")?;
    let coprocessor_cfg_path = env::var("COPROCESSOR_CFG_PATH")?;

    info!(target: RUNNER, "Using configuration files:");
    info!(target: RUNNER, "  Neutron: {}", neutron_cfg_path);
    info!(target: RUNNER, "  Ethereum: {}", ethereum_cfg_path);
    info!(target: RUNNER, "  Gaia: {}", gaia_cfg_path);
    info!(target: RUNNER, "  Co-processor: {}", coprocessor_cfg_path);

    // initialize the strategy from configuration files
    let strategy = Strategy::from_files(
        &neutron_cfg_path,
        &gaia_cfg_path,
        &ethereum_cfg_path,
        &coprocessor_cfg_path,
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
