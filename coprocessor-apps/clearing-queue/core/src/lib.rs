#![no_std]

extern crate alloc;

pub const VAULT_ADDRESS: &str = "0x9fe5b9c7ddbd26d0dc93634e15eb1a5d34c85493";

mod proof;
mod types;

pub use proof::*;
pub use types::*;
