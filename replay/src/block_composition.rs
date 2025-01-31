use std::{
    collections::HashMap,
    error::Error,
    fs::{self, File},
    path::Path,
};

use blockifier::{
    execution::call_info::CallInfo,
    transaction::{errors::TransactionExecutionError, objects::TransactionExecutionInfo},
};
use serde::Serialize;
use starknet_api::core::{ClassHash, EntryPointSelector};

type BlockExecutionInfo = Vec<(
    u64,
    String,
    Vec<Result<TransactionExecutionInfo, TransactionExecutionError>>,
)>;

#[derive(Debug, Serialize)]
struct BlockEntryPoints {
    block_number: u64,
    block_timestamp: String,
    entrypoints: Vec<BlockEntryPoint>,
}

#[derive(Debug, Serialize)]
struct BlockEntryPoint {
    validate_call_info: Option<Vec<EntryPointExecution>>,
    execute_call_info: Option<Vec<EntryPointExecution>>,
    fee_transfer_call_info: Option<Vec<EntryPointExecution>>,
}

#[derive(Debug, Serialize)]
struct EntryPointExecution {
    class_hash: ClassHash,
    selector: EntryPointSelector,
}

/// Saves to a json the resulting list of `BlockEntryPoints`
pub fn save_entry_point_execution(
    file_path: &Path,
    executions: BlockExecutionInfo,
) -> Result<(), Box<dyn Error>> {
    if let Some(parent_path) = file_path.parent() {
        fs::create_dir_all(parent_path)?;
    }

    let mut blocks: Vec<BlockEntryPoints> = Vec::new();

    for (block_number, block_timestamp, executions) in executions {
        let entrypoints = executions
            .into_iter()
            .map(|execution_rst| {
                let mut tx_execution = HashMap::new();
                let execution = execution_rst.unwrap();
                let mut block_entry_point = BlockEntrypoin {
                    validate_call_info: None,
                    execute_call_info: None,
                    fee_transfer_call_info: None,
                };

                if let Some(call) = execution.validate_call_info {
                    block_entry_point.validate_call_info = get_inner_class_executions(call);
                }
                if let Some(call) = execution.execute_call_info {
                    block_entry_point.execute_call_info = get_inner_class_executions(call);
                }
                if let Some(call) = execution.fee_transfer_call_info {
                    block_entry_point.fee_transfer_call_info = get_inner_class_executions(call);
                }

                tx_execution
            })
            .collect::<Vec<_>>();

        blocks.push(BlockEntryPoints {
            block_number,
            block_timestamp,
            entrypoints,
        });
    }

    let file = File::create(file_path)?;
    serde_json::to_writer_pretty(file, &blocks)?;

    Ok(())
}

fn get_inner_class_executions(call: CallInfo) -> Vec<EntryPointExecution> {
    // class hash can initially be None, but it is always added before execution
    let class_hash = call.call.class_hash.unwrap();

    let mut classes = call
        .inner_calls
        .into_iter()
        .flat_map(get_inner_class_executions)
        .collect::<Vec<_>>();

    let top_class = EntryPointExecution {
        class_hash,
        selector: call.call.entry_point_selector,
    };

    classes.push(top_class);

    classes
}
