use std::time::Duration;

use blockifier::{
    execution::{call_info::CallInfo, contract_class::TrackedResource},
    transaction::objects::TransactionExecutionInfo,
};
use serde::Serialize;
use starknet_api::{
    block::BlockNumber,
    core::{ClassHash, EntryPointSelector},
    transaction::TransactionHash,
};

type TransactionExecutionOutput = (TransactionHash, TransactionExecutionInfo, Duration);

#[derive(Serialize)]
pub struct BenchmarkingData {
    pub transactions: Vec<TransactionExecutionData>,
    pub calls: Vec<ClassExecutionData>,
}

#[derive(Serialize)]
pub struct ClassExecutionData {
    class_hash: ClassHash,
    selector: EntryPointSelector,
    time_ns: u128,
    gas_consumed: u64,
    steps: u64,
    resource: TrackedResource,
}

#[derive(Serialize)]
pub struct TransactionExecutionData {
    hash: TransactionHash,
    time_ns: u128,
    gas_consumed: u64,
    steps: u64,
    first_call: usize,
    block_number: u64,
}

pub fn aggregate_executions(
    executions: Vec<(BlockNumber, Vec<TransactionExecutionOutput>)>,
) -> BenchmarkingData {
    let mut calls = vec![];
    let mut transactions = vec![];

    for (block_number, executions) in executions {
        for (hash, execution, time) in executions {
            let first_class_index = calls.len();

            let mut gas_consumed = 0;
            let mut steps = 0;

            if let Some(call) = execution.validate_call_info {
                gas_consumed += call.execution.gas_consumed;
                steps += call.resources.n_steps as u64;
                calls.append(&mut get_calls(call));
            }
            if let Some(call) = execution.execute_call_info {
                gas_consumed += call.execution.gas_consumed;
                steps += call.resources.n_steps as u64;
                calls.append(&mut get_calls(call));
            }
            if let Some(call) = execution.fee_transfer_call_info {
                gas_consumed += call.execution.gas_consumed;
                steps += call.resources.n_steps as u64;
                calls.append(&mut get_calls(call));
            }

            transactions.push(TransactionExecutionData {
                hash,
                time_ns: time.as_nanos(),
                first_call: first_class_index,
                gas_consumed,
                steps,
                block_number: block_number.0,
            });
        }
    }

    BenchmarkingData {
        transactions,
        calls,
    }
}

fn get_calls(call: CallInfo) -> Vec<ClassExecutionData> {
    // class hash can initially be None, but it is always added before execution
    let class_hash = call.call.class_hash.unwrap();

    let mut inner_time = Duration::ZERO;
    let mut inner_steps = 0;
    let mut inner_gas_consumed = 0;

    let mut classes = call
        .inner_calls
        .into_iter()
        .flat_map(|call| {
            inner_time += call.time;
            inner_gas_consumed += call.execution.gas_consumed;
            inner_steps += call.resources.n_steps as u64;
            get_calls(call)
        })
        .collect::<Vec<_>>();

    let time = call
        .time
        .checked_sub(inner_time)
        .expect("time cannot be negative");

    let gas_consumed = call
        .execution
        .gas_consumed
        .checked_sub(inner_gas_consumed)
        .expect("gas cannot be negative");

    let steps = (call.resources.n_steps as u64)
        .checked_sub(inner_steps)
        .expect("gas cannot be negative");

    let top_class = ClassExecutionData {
        class_hash,
        selector: call.call.entry_point_selector,
        time_ns: time.as_nanos(),
        gas_consumed,
        resource: call.tracked_resource,
        steps,
    };

    classes.push(top_class);

    classes
}
