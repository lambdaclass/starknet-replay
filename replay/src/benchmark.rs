use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use anyhow::Context;
use blockifier::execution::call_info::CallInfo;
use cairo_lang_starknet_classes::{
    casm_contract_class::CasmContractClass,
    contract_class::{version_id_from_serialized_sierra_program, ContractClass},
};
use cairo_native::{executor::AotContractExecutor, statistics::Statistics, OptLevel};
use serde::{Deserialize, Serialize};
use starknet_api::{
    block::BlockNumber,
    core::{ClassHash, EntryPointSelector},
    transaction::TransactionHash,
};
use tracing::info;

use crate::execution::TransactionExecution;

#[derive(Serialize, Deserialize)]
pub struct BenchData {
    pub transactions: Vec<TxBenchData>,
    pub calls: Vec<CallBenchData>,
}

#[derive(Serialize, Deserialize)]
pub struct CallBenchData {
    tx_hash: TransactionHash,
    class_hash: ClassHash,
    selector: EntryPointSelector,
    time_ns: u128,
    gas_consumed: u64,
    steps: u64,
    cairo_native: bool,
}

#[derive(Serialize, Deserialize)]
pub struct TxBenchData {
    tx_hash: TransactionHash,
    block_number: BlockNumber,
    time_ns: u128,
    gas_consumed: u64,
    steps: u64,
    failed: bool,
}

impl BenchData {
    pub fn aggregate(txs: &[TransactionExecution]) -> Self {
        // Group by transaction hash
        let mut grouped_txs = BTreeMap::new();
        for tx in txs {
            grouped_txs.entry(tx.hash).or_insert_with(Vec::new).push(tx);
        }

        let mut aggregated_txs = Vec::new();
        let mut aggregated_calls = Vec::new();

        // Iterate each transaction group, and aggregate it into a single entry
        // by dividing the resource usage by the number of executions.
        for (_, txs) in grouped_txs {
            let summarized_txs = txs.into_iter().map(summarize_tx).collect::<Vec<_>>();

            let execution_count = summarized_txs.len();

            let (mut tx_data, mut calls) = summarized_txs
                .into_iter()
                .reduce(|(mut tx_data, mut calls), (other_tx_data, other_calls)| {
                    tx_data.time_ns += other_tx_data.time_ns;
                    tx_data.steps += other_tx_data.steps;
                    tx_data.gas_consumed += other_tx_data.gas_consumed;
                    for (call, other_call) in calls.iter_mut().zip(other_calls.iter()) {
                        call.time_ns += other_call.time_ns;
                        call.steps += other_call.steps;
                        call.gas_consumed += other_call.gas_consumed;
                    }
                    (tx_data, calls)
                })
                .expect("we should have at least one execution");

            tx_data.time_ns = tx_data.time_ns.div_ceil(execution_count as u128);
            tx_data.gas_consumed = tx_data.gas_consumed.div_ceil(execution_count as u64);
            tx_data.steps = tx_data.steps.div_ceil(execution_count as u64);
            for call in &mut calls {
                call.time_ns = call.time_ns.div_ceil(execution_count as u128);
                call.gas_consumed = call.gas_consumed.div_ceil(execution_count as u64);
                call.steps = call.steps.div_ceil(execution_count as u64);
            }

            aggregated_txs.push(tx_data);
            aggregated_calls.append(&mut calls);
        }

        Self {
            transactions: aggregated_txs,
            calls: aggregated_calls,
        }
    }
}

pub fn summarize_tx(tx: &TransactionExecution) -> (TxBenchData, Vec<CallBenchData>) {
    let mut tx_data = TxBenchData {
        tx_hash: tx.hash,
        block_number: tx.block_number,
        time_ns: tx.time.as_nanos(),
        gas_consumed: 0,
        steps: 0,
        failed: tx.result.is_err(),
    };

    let Ok(info) = &tx.result else {
        return (tx_data, vec![]);
    };

    let mut calls = Vec::new();

    for call_info in info.non_optional_call_infos() {
        tx_data.gas_consumed += call_info.execution.gas_consumed;
        tx_data.steps += call_info.resources.n_steps as u64;

        calls.append(&mut summarize_calls(tx.hash, call_info));
    }

    (tx_data, calls)
}

fn summarize_calls(tx_hash: TransactionHash, call: &CallInfo) -> Vec<CallBenchData> {
    // class hash can initially be None, but it is always added before execution
    let class_hash = call.call.class_hash.unwrap();

    let mut inner_time = Duration::ZERO;
    let mut inner_steps = 0;
    let mut inner_gas_consumed = 0;

    let mut classes = call
        .inner_calls
        .iter()
        .flat_map(|call| {
            inner_time += call.time;
            inner_gas_consumed += call.execution.gas_consumed;
            inner_steps += call.resources.n_steps as u64;
            summarize_calls(tx_hash, call)
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

    let top_call = CallBenchData {
        tx_hash,
        class_hash,
        selector: call.call.entry_point_selector,
        cairo_native: call.execution.cairo_native,
        time_ns: time.as_nanos(),
        gas_consumed,
        steps,
    };

    classes.push(top_call);

    classes
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClassBenchmarkSummary {
    pub class_hash: ClassHash,
    pub native_time_ns: u128,
    pub casm_time_ns: u128,
    pub sierra_statement_count: usize,
    pub object_size_bytes: usize,
    pub casm_bytecode_length: usize,
}

impl ClassBenchmarkSummary {
    pub fn aggregate(summaries: Vec<Self>) -> anyhow::Result<Self> {
        let samples = summaries.len() as u128;

        let mut summary = summaries
            .into_iter()
            .reduce(|mut summary1, summary2| {
                assert_eq!(summary1.class_hash, summary2.class_hash);
                assert_eq!(
                    summary1.sierra_statement_count,
                    summary2.sierra_statement_count
                );
                assert_eq!(summary1.object_size_bytes, summary2.object_size_bytes);
                assert_eq!(summary1.casm_bytecode_length, summary2.casm_bytecode_length);

                summary1.native_time_ns += summary2.native_time_ns;
                summary1.casm_time_ns += summary2.native_time_ns;

                summary1
            })
            .context("we should have at least one summary")?;

        summary.native_time_ns /= samples;
        summary.casm_time_ns /= samples;

        Ok(summary)
    }
}

pub fn benchmark_compilation(
    class_hash: ClassHash,
    contract_class: ContractClass,
) -> anyhow::Result<ClassBenchmarkSummary> {
    let (sierra_version, _) =
        version_id_from_serialized_sierra_program(&contract_class.sierra_program)?;

    let mut statistics = Statistics::default();

    info!(
        "compiling native contract class {}",
        class_hash.to_fixed_hex_string()
    );

    let pre_native_compilation_instant = Instant::now();
    let _ = AotContractExecutor::new(
        &contract_class.extract_sierra_program()?,
        &contract_class.entry_points_by_type,
        sierra_version,
        OptLevel::Default,
        Some(&mut statistics),
    )?;
    let native_time_ns = pre_native_compilation_instant.elapsed().as_nanos();

    info!(
        "compiling casm contract class {}",
        class_hash.to_fixed_hex_string()
    );

    let pre_casm_compilation_instant = Instant::now();
    let casm_contract_class =
        CasmContractClass::from_contract_class(contract_class, false, usize::MAX)?;
    let casm_time_ns = pre_casm_compilation_instant.elapsed().as_nanos();

    let sierra_statement_count = statistics
        .sierra_statement_count
        .context("missing sierra_statement_count statistic")?;
    let object_size_bytes = statistics
        .object_size_bytes
        .context("missing object_size_bytes statistic")?;

    Ok(ClassBenchmarkSummary {
        class_hash,
        native_time_ns,
        casm_time_ns,
        sierra_statement_count,
        object_size_bytes,
        casm_bytecode_length: casm_contract_class.bytecode.len(),
    })
}
