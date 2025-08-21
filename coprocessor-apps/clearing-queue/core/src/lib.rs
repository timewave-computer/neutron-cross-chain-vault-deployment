#![no_std]

extern crate alloc;

pub const VAULT_ADDRESS: &str = "0x3f50ac4c0d7e2d63feec9737a8ecde1fa7af41c3";

mod proof;
mod types;

pub use proof::*;
pub use types::*;
