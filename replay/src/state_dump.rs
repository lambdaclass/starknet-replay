use std::{
    collections::BTreeMap,
    error::Error,
    fs::{self, File},
    path::Path,
};

use blockifier::state::{
    cached_state::{CachedState, StateMaps, StorageEntry},
    state_api::StateReader,
};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_types_core::felt::Felt;

#[serde_as]
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct SerializableStateMaps {
    #[serde_as(as = "Vec<(_, _)>")]
    pub nonces: BTreeMap<ContractAddress, Nonce>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub class_hashes: BTreeMap<ContractAddress, ClassHash>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub storage: BTreeMap<StorageEntry, Felt>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub compiled_class_hashes: BTreeMap<ClassHash, CompiledClassHash>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub declared_contracts: BTreeMap<ClassHash, bool>,
}

impl From<StateMaps> for SerializableStateMaps {
    fn from(value: StateMaps) -> Self {
        Self {
            nonces: value.nonces.into_iter().collect(),
            class_hashes: value.class_hashes.into_iter().collect(),
            storage: value.storage.into_iter().collect(),
            compiled_class_hashes: value.compiled_class_hashes.into_iter().collect(),
            declared_contracts: value.declared_contracts.into_iter().collect(),
        }
    }
}

pub fn dump_state_diff(
    state: &mut CachedState<impl StateReader>,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let diff = SerializableStateMaps::from(state.to_state_diff()?);
    let file = File::create(path)?;

    serde_json::to_writer_pretty(file, &diff)?;

    Ok(())
}
