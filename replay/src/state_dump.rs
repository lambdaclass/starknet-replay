use std::{
    collections::BTreeMap,
    error::Error,
    fs::{self, File},
    path::Path,
};

use blockifier::{
    execution::call_info::CallInfo,
    state::{
        cached_state::{CachedState, StateMaps, StorageEntry},
        state_api::StateReader,
    },
    transaction::objects::TransactionExecutionInfo,
};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_types_core::felt::Felt;

pub fn dump_state_diff(
    state: &mut CachedState<impl StateReader>,
    execution_info: &TransactionExecutionInfo,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let state_maps = SerializableStateMaps::from(state.to_state_diff()?);
    let execution_info = SerializableExecutionInfo::new(execution_info);
    let info = Info {
        execution_info,
        state_maps,
    };

    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &info)?;

    Ok(())
}

#[derive(Serialize)]
struct Info {
    execution_info: SerializableExecutionInfo,
    state_maps: SerializableStateMaps,
}

/// From `blockifier::transaction::objects::TransactionExecutionInfo`
#[derive(Serialize)]
struct SerializableExecutionInfo {
    validate_call_info: Option<CallInfo>,
    execute_call_info: Option<CallInfo>,
    fee_transfer_call_info: Option<CallInfo>,
    reverted: Option<String>,
}

impl SerializableExecutionInfo {
    pub fn new(execution_info: &TransactionExecutionInfo) -> Self {
        Self {
            validate_call_info: execution_info.validate_call_info.clone(),
            execute_call_info: execution_info.execute_call_info.clone(),
            fee_transfer_call_info: execution_info.fee_transfer_call_info.clone(),
            reverted: execution_info.revert_error.clone(),
        }
    }
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
