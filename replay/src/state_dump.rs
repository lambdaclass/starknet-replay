use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::Path,
};

use blockifier::{
    execution::{
        call_info::{CallExecution, CallInfo},
        entry_point::{CallEntryPoint, CallType},
    },
    fee::receipt::TransactionReceipt,
    state::{
        cached_state::{CachedState, StateMaps, StorageEntry},
        state_api::StateReader,
    },
    transaction::{errors::TransactionExecutionError, objects::TransactionExecutionInfo},
};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use starknet_api::{
    contract_class::EntryPointType,
    core::{ClassHash, CompiledClassHash, ContractAddress, EntryPointSelector, Nonce},
    state::StorageKey,
    transaction::fields::Calldata,
};
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
    let execution_info = SerializableExecutionInfo::new(execution_info.clone());
    let info = Info {
        execution_info,
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
    revert_error: Option<String>,
    receipt: TransactionReceipt,
}

impl SerializableExecutionInfo {
    pub fn new(execution_info: TransactionExecutionInfo) -> Self {
        let TransactionExecutionInfo {
            validate_call_info,
            execute_call_info,
            fee_transfer_call_info,
            revert_error,
            receipt,
        } = execution_info;

        Self {
            validate_call_info: validate_call_info.clone().map(From::<CallInfo>::from),
            execute_call_info: execute_call_info.clone().map(From::<CallInfo>::from),
            fee_transfer_call_info: fee_transfer_call_info.clone().map(From::<CallInfo>::from),
            revert_error: revert_error.map(|x| x.to_string()),
            receipt,
        }
    }
}

/// From `blockifier::execution::call_info::CallInfo`
#[derive(Serialize)]
struct SerializableCallInfo {
    pub call: SerializableCallEntryPoint,
    pub execution: CallExecution,
    pub inner_calls: Vec<SerializableCallInfo>,
    pub storage_read_values: Vec<Felt>,

    // Convert HashSet to vector to avoid random order
    pub accessed_storage_keys: Vec<StorageKey>,
    pub read_class_hash_values: Vec<ClassHash>,
    // Convert HashSet to vector to avoid random order
    pub accessed_contract_addresses: Vec<ContractAddress>,
}

impl From<CallInfo> for SerializableCallInfo {
    fn from(value: CallInfo) -> Self {
        let CallInfo {
            call,
            execution,
            inner_calls,
            storage_read_values,
            accessed_storage_keys,
            read_class_hash_values,
            accessed_contract_addresses,
            resources: _resources,
            tracked_resource: _tracked_resource,
            time: _time,
        } = value;

        let mut accessed_storage_keys = accessed_storage_keys.into_iter().collect::<Vec<_>>();
        accessed_storage_keys.sort();

        let mut accessed_contract_addresses =
            accessed_contract_addresses.into_iter().collect::<Vec<_>>();
        accessed_contract_addresses.sort();

        Self {
            call: SerializableCallEntryPoint::from(call),
            execution,
            inner_calls: inner_calls
                .into_iter()
                .map(From::<CallInfo>::from)
                .collect(),
            storage_read_values,
            accessed_storage_keys,
            read_class_hash_values,
            accessed_contract_addresses,
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
    pub initial_gas: u64,
}
impl From<CallEntryPoint> for SerializableCallEntryPoint {
    fn from(value: CallEntryPoint) -> Self {
        let CallEntryPoint {
            class_hash,
            code_address,
            entry_point_type,
            entry_point_selector,
            calldata,
            storage_address,
            caller_address,
            call_type,
            initial_gas,
        } = value;
        Self {
            class_hash,
            code_address,
            entry_point_type,
            entry_point_selector,
            calldata,
            storage_address,
            caller_address,
            call_type,
            initial_gas,
        }
    }
}
