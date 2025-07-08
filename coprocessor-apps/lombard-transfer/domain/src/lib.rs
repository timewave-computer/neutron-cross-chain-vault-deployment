#![no_std]

use serde_json::Value;
use valence_coprocessor::{StateProof, ValidatedBlock};
use valence_coprocessor_wasm::abi;

pub fn validate_block(args: Value) -> anyhow::Result<ValidatedBlock> {
    abi::log!("validate block not implemented, but received {args:?}")?;

    todo!()
}

pub fn get_state_proof(args: Value) -> anyhow::Result<StateProof> {
    abi::log!("get state proof not implemented, but received {args:?}")?;

    todo!()
}
