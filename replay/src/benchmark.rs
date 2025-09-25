use std::collections::HashMap;

use blockifier::execution::call_info::CallInfo;
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

impl TxBenchmarkSummary {
    pub fn aggregate(summaries: Vec<TxBenchmarkSummary>) -> TxBenchmarkSummary {
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

    pub fn summarize_tx(tx: TransactionExecution) -> TxBenchmarkSummary {
        let mut tx_data = TxBenchmarkSummary {
            tx_hash: tx.hash,
            block_number: tx.block_number,
            time_ns: tx.time.as_nanos(),
            failed: !tx.result.as_ref().is_ok_and(|info| !info.is_reverted()),
            native_time_ns: 0,
            vm_time_ns: 0,
            native_gas: 0,
            vm_gas: 0,
        };

        let Ok(info) = &tx.result else { return tx_data };

        for call_info in info
            .non_optional_call_infos()
            .flat_map(|call_info| call_info.iter())
        {
            let (self_time, self_gas) = obtain_top_level_stats(call_info);
            if call_info.execution.cairo_native {
                tx_data.native_time_ns += self_time;
                tx_data.native_gas += self_gas;
            } else {
                tx_data.vm_time_ns += self_time;
                tx_data.vm_gas += self_gas;
            }
        }

        tx_data
    }
}

/// Returns the statistics of the given call info,
/// excluding the inner calls.
///
/// Returns a tuple with:
/// - Call duration, in nanoseconds.
/// - Gas consumed, accounting for steps.
fn obtain_top_level_stats(call_info: &CallInfo) -> (u128, u64) {
    let mut self_time = call_info.time.as_nanos();
    let mut self_gas = call_info.execution.gas_consumed;
    let mut self_steps = call_info.resources.n_steps;
    for inner_call in &call_info.inner_calls {
        self_time -= inner_call.time.as_nanos();
        self_gas -= inner_call.execution.gas_consumed;
        self_steps -= inner_call.resources.n_steps;
    }

    // A step is equivalent to 100 units of gas.
    let total_self_gas = self_gas + self_steps as u64 * 100;

    (self_time, total_self_gas)
}

pub fn add_transaction_to_execution_benchmark(
    benchmark: &mut HashMap<TransactionHash, Vec<TxBenchmarkSummary>>,
    tx: TransactionExecution,
) {
    benchmark
        .entry(tx.hash)
        .or_default()
        .push(TxBenchmarkSummary::summarize_tx(tx));
}

pub fn aggregate_execution_benchmark(
    benchmark: HashMap<TransactionHash, Vec<TxBenchmarkSummary>>,
) -> Vec<TxBenchmarkSummary> {
    benchmark
        .into_values()
        .map(TxBenchmarkSummary::aggregate)
        .collect()
}
