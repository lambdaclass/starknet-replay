use std::{
    collections::BTreeMap,
    error::Error,
    fs::{self, File},
    path::Path,
};

use blockifier::{
    execution::{
        call_info::{CallExecution, CallInfo, OrderedEvent, OrderedL2ToL1Message, Retdata},
        entry_point::{CallEntryPoint, CallType},
    },
    state::{
        cached_state::{CachedState, StateMaps, StorageEntry},
        state_api::StateReader,
    },
    transaction::objects::TransactionExecutionInfo,
};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use starknet_api::{
    core::{ClassHash, CompiledClassHash, ContractAddress, EntryPointSelector, Nonce},
    deprecated_contract_class::EntryPointType,
    state::StorageKey,
    transaction::Calldata,
};
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

/// From `blockifier::transaction::objects::TransactionExecutionInfo`
#[derive(Serialize)]
struct SerializableExecutionInfo {
    validate_call_info: Option<SerializableCallInfo>,
    execute_call_info: Option<SerializableCallInfo>,
    fee_transfer_call_info: Option<SerializableCallInfo>,
    reverted: Option<String>,
}

impl SerializableExecutionInfo {
    pub fn new(execution_info: &TransactionExecutionInfo) -> Self {
        Self {
            validate_call_info: execution_info
                .validate_call_info
                .clone()
                .map(From::<CallInfo>::from),
            execute_call_info: execution_info
                .execute_call_info
                .clone()
                .map(From::<CallInfo>::from),
            fee_transfer_call_info: execution_info
                .fee_transfer_call_info
                .clone()
                .map(From::<CallInfo>::from),
            reverted: execution_info.revert_error.clone(),
        }
    }
}

/// From `blockifier::execution::call_info::CallInfo`
#[derive(Serialize)]
struct SerializableCallInfo {
    pub call: SerializableCallEntryPoint,
    pub execution: SerializableCallExecution,
    pub inner_calls: Vec<SerializableCallInfo>,
    pub storage_read_values: Vec<Felt>,
    // Convert HashSet to vector to avoid random order
    pub accessed_storage_keys: Vec<StorageKey>,
}

impl From<CallInfo> for SerializableCallInfo {
    fn from(value: CallInfo) -> Self {
        let mut accessed_storage_keys = value.accessed_storage_keys.into_iter().collect::<Vec<_>>();
        accessed_storage_keys.sort();

        Self {
            call: value.call.into(),
            execution: value.execution.into(),
            inner_calls: value
                .inner_calls
                .into_iter()
                .map(From::<CallInfo>::from)
                .collect(),
            storage_read_values: value.storage_read_values,
            accessed_storage_keys,
        }
    }
}

/// From `blockifier::execution::entry_point::CallEntryPoint`
#[derive(Serialize)]
struct SerializableCallEntryPoint {
    pub class_hash: Option<ClassHash>,
    pub code_address: Option<ContractAddress>,
    pub entry_point_type: EntryPointType,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
    pub storage_address: ContractAddress,
    pub caller_address: ContractAddress,
    pub call_type: CallType,
    // Ignore gas
    // pub initial_gas: u64,
}
impl From<CallEntryPoint> for SerializableCallEntryPoint {
    fn from(value: CallEntryPoint) -> Self {
        Self {
            class_hash: value.class_hash,
            code_address: value.code_address,
            entry_point_type: value.entry_point_type,
            entry_point_selector: value.entry_point_selector,
            calldata: value.calldata,
            storage_address: value.storage_address,
            caller_address: value.caller_address,
            call_type: value.call_type,
        }
    }
}

/// From `blockifier::execution::call_info::CallExecution`
#[derive(Serialize)]
struct SerializableCallExecution {
    pub retdata: Retdata,
    pub events: Vec<OrderedEvent>,
    pub l2_to_l1_messages: Vec<OrderedL2ToL1Message>,
    pub failed: bool,
    // Ignore gas
    // pub initial_gas: u64,
}
impl From<CallExecution> for SerializableCallExecution {
    fn from(value: CallExecution) -> Self {
        Self {
            retdata: value.retdata,
            events: value.events,
            l2_to_l1_messages: value.l2_to_l1_messages,
            failed: value.failed,
        }
    }
}
