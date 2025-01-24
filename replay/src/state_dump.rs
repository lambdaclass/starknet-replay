use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::Path,
};

use blockifier::{
    state::{
        cached_state::{CachedState, StateMaps, StorageEntry},
        state_api::StateReader,
    },
    transaction::{errors::TransactionExecutionError, objects::TransactionExecutionInfo},
};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_types_core::felt::Felt;

pub fn dump_state_diff(
    state: &mut CachedState<impl StateReader>,
    execution_info: &TransactionExecutionInfo,
    path: &Path,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let state_maps = SerializableStateMaps::from(state.to_state_diff()?.state_maps);
    let info = Info {
        execution_info: execution_info.clone(),
        state_maps,
    };

    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &info)?;

    Ok(())
}

pub fn dump_error(err: &TransactionExecutionError, path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let info = ErrorInfo {
        reverted: err.to_string(),
    };

    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &info)?;

    Ok(())
}

// The error messages is different between CairoVM and Cairo Native. That is way
// we must ignore them while comparing the state dumps. To make ignoring them
// easier, we name the field that contains the error message as "reverted" both
// in `Info` and `ErrorInfo`. That way we can just filter out that line before
// comparing them

#[derive(Serialize)]
struct ErrorInfo {
    reverted: String,
}

#[derive(Serialize)]
struct Info {
    execution_info: TransactionExecutionInfo,
    state_maps: SerializableStateMaps,
}

/// From `blockifier::state::cached_state::StateMaps`
#[serde_as]
#[derive(Serialize, Deserialize)]
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
