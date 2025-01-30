use std::{
    collections::HashMap,
    error::Error,
    fs::{self, File},
    path::{Path, PathBuf},
};

use blockifier::{execution::call_info::CallInfo, transaction::objects::TransactionExecutionInfo};
use serde::Serialize;
use starknet_api::core::{ClassHash, EntryPointSelector};

#[derive(Debug, Serialize)]
struct BlockEntryPoints {
    block_number: u64,
    block_timestamp: String,
    entrypoints: Vec<HashMap<String, Vec<EntryPointExecution>>>,
}

#[derive(Debug, Serialize)]
struct EntryPointExecution {
    class_hash: ClassHash,
    selector: EntryPointSelector,
}

pub fn save_entry_point_execution(
    file_path: &Path,
    executions: Vec<(u64, String, Vec<TransactionExecutionInfo>)>,
) -> Result<(), Box<dyn Error>> {
    let mut blocks: Vec<BlockEntryPoints> = Vec::new();
    let dir_path = PathBuf::from("block-range_entrypoints_execution");
    let _ = fs::create_dir_all(&dir_path);
    let path = dir_path.join(format!("/{}", file_path.display()));

    for (block_number, block_timestamp, executions) in executions {
        let entrypoints = executions
            .into_iter()
            .map(|execution| {
                let mut tx_execution = HashMap::new();

                if let Some(call) = execution.validate_call_info {
                    tx_execution.insert(
                        "validate_call_info".to_string(),
                        get_inner_class_executions(call),
                    );
                }
                if let Some(call) = execution.execute_call_info {
                    tx_execution.insert(
                        "execute_call_info".to_string(),
                        get_inner_class_executions(call),
                    );
                }
                if let Some(call) = execution.fee_transfer_call_info {
                    tx_execution.insert(
                        "fee_transfer_call_info".to_string(),
                        get_inner_class_executions(call),
                    );
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

    let file = File::create(path)?;
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
