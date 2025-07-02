use std::env;
use log::info;
use opentelemetry_otlp::{WithExportConfig, Protocol};
use opentelemetry_appender_log::{OpenTelemetryLogBridge};
use opentelemetry_sdk::logs::{BatchLogProcessor, SdkLoggerProvider};
use opentelemetry_sdk::{Resource};
use multi_log;

const LOGGING: &str = "logging";
pub async fn setup_logging() -> anyhow::Result<()> {
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
            multi_log::MultiLogger::init(vec![otlp_logger, std_logger], log::Level::Trace)?;

        },
        Err(_) => {
            env_logger::init();
            info!(target: LOGGING, "OTLP_ENDPOINT not set, skipping OpenTelemetry logging");
        }
    };

    Ok(())

}
