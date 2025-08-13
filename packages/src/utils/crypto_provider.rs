use rustls::crypto::{aws_lc_rs, CryptoProvider};

pub async fn setup_crypto_provider() -> anyhow::Result<()> {
    match CryptoProvider::install_default(aws_lc_rs::default_provider()) {
        Ok(()) => println!("Crypto provider installed"),
        Err(_) => println!("Crypto provider already installed"),
    }

    Ok(())
}
