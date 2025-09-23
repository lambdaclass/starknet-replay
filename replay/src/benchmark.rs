use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use starknet_api::{block::BlockNumber, transaction::TransactionHash};

use crate::execution::TransactionExecution;

#[derive(Serialize, Deserialize)]
pub struct TxBenchmarkSummary {
    pub tx_hash: TransactionHash,
    pub block_number: BlockNumber,
    pub time_ns: u128,
    pub native_time_ns: u128,
    pub vm_time_ns: u128,
    pub native_gas: u64,
    pub vm_gas: u64,
    pub failed: bool,
}

pub fn add_transaction_to_benchmark(
    benchmark: &mut HashMap<TransactionHash, Vec<TxBenchmarkSummary>>,
    tx: TransactionExecution,
) {
    benchmark.entry(tx.hash).or_default().push(summarize_tx(tx));
}

pub fn aggregate_benchmark(
    benchmark: HashMap<TransactionHash, Vec<TxBenchmarkSummary>>,
) -> Vec<TxBenchmarkSummary> {
    benchmark.into_values().map(aggregate_summaries).collect()
}

fn summarize_tx(tx: TransactionExecution) -> TxBenchmarkSummary {
    let mut tx_data = TxBenchmarkSummary {
        tx_hash: tx.hash,
        block_number: tx.block_number,
        time_ns: tx.time.as_nanos(),
        failed: tx.result.is_err(),
        native_time_ns: 0,
        vm_time_ns: 0,
        native_gas: 0,
        vm_gas: 0,
    };

    let Ok(info) = &tx.result else { return tx_data };

    for call_info in info.non_optional_call_infos() {
        if call_info.execution.cairo_native {
            tx_data.native_time_ns += call_info.time.as_nanos();
            tx_data.native_gas +=
                call_info.execution.gas_consumed + call_info.resources.n_steps as u64 * 100;
        } else {
            tx_data.vm_time_ns += call_info.time.as_nanos();
            tx_data.vm_gas +=
                call_info.execution.gas_consumed + call_info.resources.n_steps as u64 * 100;
        }
    }

    tx_data
}

fn aggregate_summaries(summaries: Vec<TxBenchmarkSummary>) -> TxBenchmarkSummary {
    let samples = summaries.len() as u128;

    let mut summary = summaries
        .into_iter()
        .reduce(|mut summary1, summary2| {
            assert_eq!(summary1.tx_hash, summary2.tx_hash);
            assert_eq!(summary1.block_number, summary2.block_number);
            assert_eq!(summary1.native_gas, summary2.native_gas);
            assert_eq!(summary1.vm_gas, summary2.vm_gas);
            assert_eq!(summary1.failed, summary2.failed);

            summary1.time_ns += summary2.time_ns;
            summary1.native_time_ns += summary2.native_time_ns;
            summary1.vm_time_ns += summary2.vm_time_ns;

            summary1
        })
        .expect("we should have at least one summary");

    summary.time_ns /= samples;
    summary.native_time_ns /= samples;
    summary.vm_time_ns /= samples;

    summary
}
