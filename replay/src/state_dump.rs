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
    fee::{
        receipt::TransactionReceipt,
        resources::{ComputationResources, StarknetResources, TransactionResources},
    },
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
    execution_resources::{GasAmount, GasVector},
    state::StorageKey,
    transaction::fields::{Calldata, Fee},
};
use starknet_types_core::felt::Felt;
use tracing::error;

pub fn create_state_dump(
    state: &mut CachedState<impl StateReader>,
    block_number: u64,
    tx_hash_str: &str,
    execution_info_result: &Result<TransactionExecutionInfo, TransactionExecutionError>,
) {
    use std::path::Path;

    let root = if cfg!(feature = "only_cairo_vm") {
        Path::new("state_dumps/vm")
    } else if cfg!(feature = "with-sierra-emu") {
        Path::new("state_dumps/emu")
    } else {
        Path::new("state_dumps/native")
    };
    let root = root.join(format!("block{}", block_number));

    std::fs::create_dir_all(&root).ok();

    let mut path = root.join(tx_hash_str);
    path.set_extension("json");

    match execution_info_result {
        Ok(execution_info) => {
            dump_state_diff(state, execution_info, &path)
                .inspect_err(|err| error!("failed to dump state diff: {err}"))
                .ok();
        }
        Err(err) => {
            // If we have no execution info, we write the error
            // to a file so that it can be compared anyway
            dump_error(err, &path)
                .inspect_err(|err| error!("failed to dump state diff: {err}"))
                .ok();
        }
    }
}

fn dump_state_diff(
    state: &mut CachedState<impl StateReader>,
    execution_info: &TransactionExecutionInfo,
    path: &Path,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let state_maps = SerializableStateMaps::from(state.to_state_diff()?.state_maps);
    let execution_info = SerializableExecutionInfo::new(execution_info.clone());
    #[derive(Serialize)]
    struct Info {
        execution_info: SerializableExecutionInfo,
        state_maps: SerializableStateMaps,
    }
    let info = Info {
        execution_info,
        state_maps,
    };

    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &info)?;

    Ok(())
}

fn dump_error(err: &TransactionExecutionError, path: &Path) -> anyhow::Result<()> {
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

pub fn create_call_state_dump(
    state: &mut CachedState<impl StateReader>,
    tx: &str,
    call_info: &CallInfo,
) -> anyhow::Result<()> {
    use std::path::Path;

    let root = if cfg!(feature = "only_cairo_vm") {
        Path::new("call_state_dumps/vm")
    } else if cfg!(feature = "with-sierra-emu") {
        Path::new("call_state_dumps/emu")
    } else {
        Path::new("call_state_dumps/native")
    };

    std::fs::create_dir_all(&root).ok();

    let mut path = root.join(tx);
    path.set_extension("json");

    let state_maps = SerializableStateMaps::from(state.to_state_diff()?.state_maps);
    let call_info = SerializableCallInfo::from(call_info.clone());

    #[derive(Serialize)]
    struct Info {
        call_info: SerializableCallInfo,
        state_maps: SerializableStateMaps,
    }
    let info = Info {
        call_info,
        state_maps,
    };

    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &info)?;

    return Ok(());
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
    receipt: SerializableTransactionReceipt,
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
            reverted: revert_error.map(|x| x.to_string()),
            receipt: SerializableTransactionReceipt::from(receipt),
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
    pub call_counter: usize,
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
            call_counter,
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
            call_counter,
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

#[derive(Serialize)]
pub struct SerializableTransactionReceipt {
    pub fee: Fee,
    pub gas: GasVector,
    pub da_gas: GasVector,
    pub resources: SerializableTransactionResources,
}

#[derive(Serialize)]
pub struct SerializableTransactionResources {
    pub starknet_resources: StarknetResources,
    pub computation: SerializableComputationResources,
}

#[derive(Serialize)]
pub struct SerializableComputationResources {
    pub n_reverted_steps: usize,
    pub sierra_gas: GasAmount,
    pub reverted_sierra_gas: GasAmount,
}

impl From<TransactionReceipt> for SerializableTransactionReceipt {
    fn from(value: TransactionReceipt) -> Self {
        let TransactionReceipt {
            fee,
            gas,
            da_gas,
            resources:
                TransactionResources {
                    starknet_resources,
                    computation:
                        ComputationResources {
                            vm_resources: _vm_resources,
                            n_reverted_steps,
                            sierra_gas,
                            reverted_sierra_gas,
                        },
                },
        } = value;
        Self {
            fee,
            gas,
            da_gas,
            resources: SerializableTransactionResources {
                starknet_resources,
                computation: SerializableComputationResources {
                    n_reverted_steps,
                    sierra_gas,
                    reverted_sierra_gas,
                },
            },
        }
    }
}
