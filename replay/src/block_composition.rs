use std::{collections::HashMap, error::Error, fs::File, path::Path};

use blockifier::{execution::call_info::CallInfo, transaction::objects::TransactionExecutionInfo};
use serde::Serialize;
use starknet_api::core::{ClassHash, EntryPointSelector};

#[derive(Debug, Serialize)]
struct EntryPointExecution {
    class_hash: ClassHash,
    selector: EntryPointSelector,
}

pub fn save_entry_point_execution(
    path: &Path,
    executions: Vec<(u64, TransactionExecutionInfo)>,
) -> Result<(), Box<dyn Error>> {
    let mut block_executions: HashMap<u64, Vec<HashMap<String, _>>> = HashMap::new();

    for (block_number, execution) in executions {
        let mut tx_execution: HashMap<String, _> = HashMap::new();

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

        block_executions
            .entry(block_number)
            .or_insert_with(Vec::new)
            .push(tx_execution);
    }

    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &block_executions)?;

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
