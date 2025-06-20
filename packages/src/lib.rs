//! Package artifacts and types for the neutron cross-chain vault deployment
//!
//! This crate contains:
//! - Contract artifacts (CosmWasm .wasm files and Solidity .sol files)
//! - Generated Solidity types using alloy::sol! macro
//! - Contract deployment utilities

pub mod contracts;
pub mod types;

// Re-export commonly used types
pub use types::sol_types::*;
