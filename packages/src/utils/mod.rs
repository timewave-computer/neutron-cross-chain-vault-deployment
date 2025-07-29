use valence_domain_clients::coprocessor::base_client::{Base64, Proof};

pub mod logging;
pub mod mars;
pub mod maxbtc;
pub mod obligation;
pub mod skip;
pub mod supervaults;
pub mod valence_core;

/// Decodes the base64 bytes of the proof and public inputs.
pub fn decode(a: Proof) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let proof = Base64::decode(&a.proof)?;
    let inputs = Base64::decode(&a.inputs)?;

    Ok((proof, inputs))
}
