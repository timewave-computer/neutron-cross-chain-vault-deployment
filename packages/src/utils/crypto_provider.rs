use rustls::crypto::{aws_lc_rs, CryptoProvider};

pub async fn setup_crypto_provider() -> anyhow::Result<()> {
    CryptoProvider::install_default(aws_lc_rs::default_provider())
        .expect("Failed to install rustls crypto provider");

    Ok(())
}
