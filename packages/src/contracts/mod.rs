use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const PATH_NEUTRON_CODE_IDS: &str = "packages/src/contracts/neutron_code_ids.toml";

#[derive(Deserialize, Serialize)]
pub struct UploadedContracts {
    pub code_ids: HashMap<String, u64>,
}
