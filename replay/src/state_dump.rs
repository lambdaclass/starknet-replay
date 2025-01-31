use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::Path,
};

use blockifier::{
    execution::{
        call_info::{CallInfo, OrderedEvent, OrderedL2ToL1Message, Retdata},
        entry_point::{CallEntryPoint, CallType},
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
    execution_resources::GasVector,
    state::StorageKey,
    transaction::fields::Calldata,
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
    let execution_info = SerializableExecutionInfo::new(execution_info);
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
    receipt: SerializableTransactionReceipt,
    reverted: Option<String>,
}

impl SerializableExecutionInfo {
    pub fn new(execution_info: &TransactionExecutionInfo) -> Self {
        let reverted = execution_info.revert_error.clone().map(|f| f.to_string());
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
            reverted,
            receipt: SerializableTransactionReceipt {
                resources: SerializableTransactionResources {
                    starknet_resources: SerializableStarknetResources {
                        calldata_length: execution_info
                            .receipt
                            .resources
                            .starknet_resources
                            .archival_data
                            .calldata_length,
                        state_changes_for_fee: SerializableStateChangesCount {
                            n_storage_updates: execution_info
                                .receipt
                                .resources
                                .starknet_resources
                                .state
                                .state_changes_for_fee
                                .state_changes_count
                                .n_storage_updates,
                            n_class_hash_updates: execution_info
                                .receipt
                                .resources
                                .starknet_resources
                                .state
                                .state_changes_for_fee
                                .state_changes_count
                                .n_class_hash_updates,
                            n_compiled_class_hash_updates: execution_info
                                .receipt
                                .resources
                                .starknet_resources
                                .state
                                .state_changes_for_fee
                                .state_changes_count
                                .n_compiled_class_hash_updates,
                            n_modified_contracts: execution_info
                                .receipt
                                .resources
                                .starknet_resources
                                .state
                                .state_changes_for_fee
                                .state_changes_count
                                .n_modified_contracts,
                        },
                        message_cost_info: SerializableMessageL1CostInfo {
                            l2_to_l1_payload_lengths: execution_info
                                .receipt
                                .resources
                                .starknet_resources
                                .messages
                                .l2_to_l1_payload_lengths
                                .clone(),
                            message_segment_length: execution_info
                                .receipt
                                .resources
                                .starknet_resources
                                .messages
                                .message_segment_length,
                        },
                        l1_handler_payload_size: execution_info
                            .receipt
                            .resources
                            .starknet_resources
                            .messages
                            .l1_handler_payload_size,
                        n_events: execution_info
                            .receipt
                            .resources
                            .starknet_resources
                            .archival_data
                            .event_summary
                            .n_events,
                    },
                },
                da_gas: execution_info.receipt.da_gas,
            },
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
            call: SerializableCallEntryPoint {
                class_hash: value.call.class_hash,
                code_address: value.call.code_address,
                entry_point_type: value.call.entry_point_type,
                entry_point_selector: value.call.entry_point_selector,
                calldata: value.call.calldata,
                storage_address: value.call.storage_address,
                caller_address: value.call.caller_address,
                call_type: value.call.call_type,
            },
            execution: SerializableCallExecution {
                retdata: value.execution.retdata,
                events: value.execution.events,
                l2_to_l1_messages: value.execution.l2_to_l1_messages,
                failed: value.execution.failed,
            },
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

/// From `blockifier::fee::actual_cost::TransactionReceipt`
#[derive(Serialize)]
pub struct SerializableTransactionReceipt {
    pub resources: SerializableTransactionResources,
    pub da_gas: GasVector,
    // Ignore gas
    // pub fee: Fee,
    // pub gas: GasVector,
}

#[derive(Serialize)]
pub struct SerializableTransactionResources {
    pub starknet_resources: SerializableStarknetResources,
    // Ignore only vm fields
    // pub n_reverted_steps: usize,
    // pub vm_resources: ExecutionResources,
}

#[derive(Serialize)]
pub struct SerializableStarknetResources {
    pub calldata_length: usize,
    pub state_changes_for_fee: SerializableStateChangesCount,
    pub message_cost_info: SerializableMessageL1CostInfo,
    pub l1_handler_payload_size: Option<usize>,
    pub n_events: usize,
}

#[derive(Serialize)]
pub struct SerializableStateChangesCount {
    pub n_storage_updates: usize,
    pub n_class_hash_updates: usize,
    pub n_compiled_class_hash_updates: usize,
    pub n_modified_contracts: usize,
    // Remaining fields where private
    // signature_length: usize,
    // code_size: usize,
    // total_event_keys: u128,
    // total_event_data_size: u128,
}

#[derive(Serialize)]
pub struct SerializableMessageL1CostInfo {
    pub l2_to_l1_payload_lengths: Vec<usize>,
    pub message_segment_length: usize,
}
