use blockifier::execution::contract_class::TrackedResource;
use blockifier::execution::entry_point::{
    EntryPointExecutionContext, EntryPointRevertInfo, ExecutableCallEntryPoint,
    SierraGasRevertTracker,
};
use blockifier::execution::execution_utils::execute_entry_point_call;
use blockifier::state::cached_state::CachedState;
use blockifier::state::state_api::StateReader as _;
use blockifier::transaction::account_transaction::ExecutionFlags;
use blockifier::transaction::objects::{RevertError, TransactionExecutionInfo};
use blockifier::transaction::transactions::ExecutableTransaction;
use clap::{Parser, Subcommand};

use rpc_state_reader::cache::RpcCachedStateReader;
use rpc_state_reader::execution::fetch_transaction_with_state;
use rpc_state_reader::objects::RpcTransactionReceipt;
use rpc_state_reader::reader::{RpcStateReader, StateReader};
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_api::execution_resources::GasAmount;

use starknet_api::felt;
use starknet_api::transaction::{TransactionExecutionStatus, TransactionHash};
use tracing::{debug, error, info, info_span};
use tracing_subscriber::{util::SubscriberInitExt, EnvFilter};

#[cfg(feature = "benchmark")]
use {
    crate::benchmark::{
        aggregate_executions, execute_block_range, fetch_block_range_data, fetch_transaction_data,
        BenchmarkingData,
    },
    std::time::Instant,
};
#[cfg(feature = "block-composition")]
use {
    block_composition::save_entry_point_execution, chrono::DateTime,
    rpc_state_reader::execution::fetch_block_context,
};

use std::collections::HashSet;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(feature = "profiling")]
use {std::thread, std::time::Duration};

#[cfg(feature = "benchmark")]
mod benchmark;
#[cfg(feature = "block-composition")]
mod block_composition;
#[cfg(feature = "state_dump")]
mod state_dump;

#[derive(Debug, Parser)]
#[command(about = "Replay is a tool for executing Starknet transactions.", long_about = None)]
struct ReplayCLI {
    #[command(subcommand)]
    subcommand: ReplayExecute,
}

#[derive(Subcommand, Debug)]
enum ReplayExecute {
    #[clap(about = "Execute a single transaction given a transaction hash.")]
    Tx {
        tx_hash: String,
        chain: String,
        block_number: u64,
        #[arg(short, long)]
        charge_fee: bool,
    },
    #[clap(about = "Execute a set of transactions from a given block.")]
    BlockTxs {
        chain: String,
        block_number: u64,
        txs: Vec<String>,
        #[arg(short, long)]
        charge_fee: bool,
    },
    #[clap(about = "Execute all the transactions in a given block.")]
    Block {
        chain: String,
        block_number: u64,
        #[arg(short, long)]
        charge_fee: bool,
    },
    #[clap(about = "Execute all the transactions in a given range of blocks.")]
    BlockRange {
        block_start: u64,
        block_end: u64,
        chain: String,
        #[arg(short, long)]
        charge_fee: bool,
    },
    #[cfg(feature = "benchmark")]
    #[clap(
        about = "Measures the time it takes to run all transactions in a given range of blocks.
Caches all rpc data before the benchmark runs to provide accurate results"
    )]
    BenchBlockRange {
        block_start: u64,
        block_end: u64,
        chain: String,
        number_of_runs: usize,
        #[arg(short, long, default_value=PathBuf::from("data").into_os_string())]
        output: PathBuf,
    },
    #[cfg(feature = "benchmark")]
    #[clap(about = "Measures the time it takes to run a single transaction.
        Caches all rpc data before the benchmark runs to provide accurate results.
        It only works if the transaction doesn't depend on another transaction in the same block")]
    BenchTx {
        tx: String,
        chain: String,
        block: u64,
        number_of_runs: usize,
        #[arg(short, long, default_value=PathBuf::from("data").into_os_string())]
        output: PathBuf,
    },
    #[cfg(feature = "block-composition")]
    #[clap(
        about = "Executes a range of blocks and writes down to a file every entrypoint executed."
    )]
    BlockCompose {
        block_start: u64,
        block_end: u64,
        chain: String,
    },
    Call {
        call_path: PathBuf,
        tx: String,
        block_number: u64,
        chain: String,
    },
}

fn main() {
    dotenvy::dotenv().ok();
    set_global_subscriber();

    let cli = ReplayCLI::parse();
    match cli.subcommand {
        ReplayExecute::Tx {
            tx_hash,
            chain,
            block_number,
            charge_fee,
        } => {
            let mut state = build_cached_state(&chain, block_number - 1);
            let reader = build_reader(&chain, block_number);

            show_execution_data(
                &mut state,
                &reader,
                tx_hash,
                &chain,
                block_number,
                charge_fee,
            );
        }
        ReplayExecute::Block {
            block_number,
            chain,
            charge_fee,
        } => {
            let _block_span = info_span!("block", number = block_number).entered();

            let mut state = build_cached_state(&chain, block_number - 1);
            let reader = build_reader(&chain, block_number);

            let transaction_hashes = reader
                .get_block_with_tx_hashes()
                .expect("Unable to fetch the transaction hashes.")
                .transactions;
            for tx_hash in transaction_hashes {
                show_execution_data(
                    &mut state,
                    &reader,
                    tx_hash.0.to_hex_string(),
                    &chain,
                    block_number,
                    charge_fee,
                );
            }
        }
        ReplayExecute::BlockTxs {
            chain,
            block_number,
            txs,
            charge_fee,
        } => {
            let _block_span = info_span!("block", number = block_number).entered();

            let txs: HashSet<TransactionHash> = HashSet::from_iter(
                txs.into_iter()
                    .map(|hash| TransactionHash(felt!(hash.as_str()))),
            );

            let mut state = build_cached_state(&chain, block_number - 1);
            let reader = build_reader(&chain, block_number);

            let transaction_hashes = reader
                .get_block_with_tx_hashes()
                .expect("Unable to fetch the transaction hashes.")
                .transactions
                .into_iter()
                .filter(|tx| txs.contains(tx));
            for tx_hash in transaction_hashes {
                show_execution_data(
                    &mut state,
                    &reader,
                    tx_hash.0.to_hex_string(),
                    &chain,
                    block_number,
                    charge_fee,
                );
            }
        }
        ReplayExecute::BlockRange {
            block_start,
            block_end,
            chain,
            charge_fee,
        } => {
            info!("executing block range: {} - {}", block_start, block_end);

            for block_number in block_start..=block_end {
                let _block_span = info_span!("block", number = block_number).entered();

                let mut state = build_cached_state(&chain, block_number - 1);
                let reader = build_reader(&chain, block_number);

                let transaction_hashes = reader
                    .get_block_with_tx_hashes()
                    .expect("Unable to fetch the transaction hashes.")
                    .transactions;
                for tx_hash in transaction_hashes {
                    show_execution_data(
                        &mut state,
                        &reader,
                        tx_hash.0.to_hex_string(),
                        &chain,
                        block_number,
                        charge_fee,
                    );
                }
            }
        }
        #[cfg(feature = "benchmark")]
        ReplayExecute::BenchBlockRange {
            block_start,
            block_end,
            chain,
            number_of_runs,
            output,
        } => {
            let block_start = BlockNumber(block_start);
            let block_end = BlockNumber(block_end);
            let chain = parse_network(&chain);

            let mut block_range_data = {
                let _caching_span = info_span!("caching block range").entered();

                info!("fetching block range data");
                let mut block_range_data = fetch_block_range_data(block_start, block_end, chain);

                // We must execute the block range once first to ensure that all data required by blockifier is cached
                info!("filling up execution cache");
                execute_block_range(&mut block_range_data);

                // Benchmark run should make no api requests as all data is cached
                // To ensure this, we disable the inner StateReader
                for (cached_state, ..) in &mut block_range_data {
                    cached_state.state.disable();
                }

                block_range_data
            };

            // We pause the main thread to differentiate
            // caching from benchmarking from within a profiler
            #[cfg(feature = "profiling")]
            thread::sleep(Duration::from_secs(1));

            {
                let _benchmark_span = info_span!("benchmarking block range").entered();

                let mut executions = Vec::new();

                info!("executing block range");
                let before_execution = Instant::now();
                for _ in 0..number_of_runs {
                    executions.push(execute_block_range(&mut block_range_data));
                }
                let execution_time = before_execution.elapsed();

                info!("saving execution info");

                let executions = executions.into_iter().flatten().collect::<Vec<_>>();
                let class_executions = aggregate_executions(executions);

                let average_time = execution_time.div_f32(number_of_runs as f32);

                let benchmarking_data = BenchmarkingData {
                    average_time,
                    class_executions,
                };

                let file = std::fs::File::create(output).unwrap();
                serde_json::to_writer_pretty(file, &benchmarking_data).unwrap();

                info!(
                    block_start = block_start.0,
                    block_end = block_end.0,
                    number_of_runs,
                    total_run_time = execution_time.as_secs_f64(),
                    average_run_time = average_time.as_secs_f64(),
                    "benchmark finished",
                );
            }
        }
        #[cfg(feature = "benchmark")]
        ReplayExecute::BenchTx {
            tx,
            block,
            chain,
            number_of_runs,
            output,
        } => {
            let chain = parse_network(&chain);
            let block = BlockNumber(block);

            let mut block_range_data = {
                let _caching_span = info_span!("caching block range").entered();

                info!("fetching transaction data");
                let transaction_data = fetch_transaction_data(&tx, block, chain);

                // We insert it into a vector so that we can reuse `execute_block_range`
                let mut block_range_data = vec![transaction_data];

                // We must execute the block range once first to ensure that all data required by blockifier is chached
                info!("filling up execution cache");
                execute_block_range(&mut block_range_data);

                // Benchmark run should make no api requests as all data is cached
                // To ensure this, we disable the inner StateReader
                for (cached_state, ..) in &mut block_range_data {
                    cached_state.state.disable();
                }

                block_range_data
            };

            // We pause the main thread to differentiate
            // caching from benchmarking from within a profiler
            #[cfg(feature = "profiling")]
            thread::sleep(Duration::from_secs(1));

            {
                let _benchmark_span = info_span!("benchmarking block range").entered();

                let mut executions = Vec::new();

                info!("executing block range");
                let before_execution = Instant::now();
                for _ in 0..number_of_runs {
                    executions.push(execute_block_range(&mut block_range_data));
                }
                let execution_time = before_execution.elapsed();

                info!("saving execution info");

                let executions = executions.into_iter().flatten().collect::<Vec<_>>();
                let class_executions = aggregate_executions(executions);

                let average_time = execution_time.div_f32(number_of_runs as f32);

                let benchmarking_data = BenchmarkingData {
                    average_time,
                    class_executions,
                };

                let file = std::fs::File::create(output).unwrap();
                serde_json::to_writer_pretty(file, &benchmarking_data).unwrap();

                info!(
                    tx = tx,
                    block = block.0,
                    number_of_runs,
                    total_run_time = execution_time.as_secs_f64(),
                    average_run_time = average_time.as_secs_f64(),
                    "benchmark finished",
                );
            }
        }
        #[cfg(feature = "block-composition")]
        ReplayExecute::BlockCompose {
            block_start,
            block_end,
            chain,
        } => {
            info!("executing block range: {} - {}", block_start, block_end);

            let mut block_executions = Vec::new();

            for block_number in block_start..=block_end {
                let _block_span = info_span!("block", number = block_number).entered();

                let mut state = build_cached_state(&chain, block_number - 1);
                let reader = build_reader(&chain, block_number);

                let flags = ExecutionFlags {
                    only_query: false,
                    charge_fee: false,
                    validate: true,
                };

                let block_context = fetch_block_context(&reader).unwrap();

                // fetch and execute transactions
                let entrypoints = reader
                    .get_block_with_tx_hashes()
                    .unwrap()
                    .transactions
                    .into_iter()
                    .map(|hash| {
                        let (tx, _) =
                            fetch_transaction_with_state(&reader, &hash, flags.clone()).unwrap();
                        let execution = tx.execute(&mut state, &block_context);
                        #[cfg(feature = "state_dump")]
                        state_dump::create_state_dump(
                            &mut state,
                            block_number,
                            &hash.to_string(),
                            &execution,
                        );
                        execution
                    })
                    .collect::<Vec<_>>();

                let block_timestamp = DateTime::from_timestamp(
                    block_context.block_info().block_timestamp.0 as i64,
                    0,
                )
                .unwrap()
                .to_string();

                block_executions.push((block_number, block_timestamp, entrypoints));
            }

            let path = PathBuf::from(format!(
                "block_composition/block-compose-{}-{}-{}.json",
                block_start, block_end, chain
            ));

            save_entry_point_execution(&path, block_executions).unwrap();
        }
        ReplayExecute::Call {
            call_path,
            tx,
            block_number,
            chain,
        } => {
            let mut state = build_cached_state(&chain, block_number - 1);
            let reader = build_reader(&chain, block_number);

            let call_file = File::open(call_path).unwrap();
            let call: ExecutableCallEntryPoint = serde_json::from_reader(call_file).unwrap();

            // We fetch the compile class from the next block, instead of the current block.
            // This ensures that it always exists.
            let compiled_class = reader.get_compiled_class(call.class_hash).unwrap();

            // This mocked context was built from trial and error. It only sets
            // the required fields to execute a sample transaction, but probably
            // won't work in every scenario. For example, if a transaction
            // depends on a particular value of the context, it would probably
            // fail.
            //
            // The actual solution is to save the exact execution context from
            // the original complete execution, and use it here, restoring every
            // single field. This was not chosen as the current implementation for the sake of simplicity, but it
            // may be a valid approach in the future.
            let mut context = {
                let tx_hash = TransactionHash(felt!(tx.as_str()));
                let flags = ExecutionFlags {
                    only_query: false,
                    charge_fee: false,
                    validate: true,
                };
                let (tx, block_context) =
                    fetch_transaction_with_state(&reader, &tx_hash, flags).unwrap();

                let tx_context = Arc::new(block_context.to_tx_context(&tx));
                let mut context = EntryPointExecutionContext::new_invoke(
                    tx_context,
                    false,
                    SierraGasRevertTracker::new(GasAmount::MAX),
                );

                let storage_class_hash = state.get_class_hash_at(call.storage_address).unwrap();

                context.revert_infos.0.push(EntryPointRevertInfo::new(
                    call.storage_address,
                    storage_class_hash,
                    context.n_emitted_events,
                    context.n_sent_messages_to_l1,
                ));

                context
                    .tracked_resource_stack
                    .push(TrackedResource::SierraGas);

                context
            };

            let call_info_result =
                execute_entry_point_call(call, compiled_class, &mut state, &mut context);

            let call_info = match call_info_result {
                Ok(x) => x,
                Err(err) => {
                    error!("execution failed: {}", err);
                    return;
                }
            };

            info!(
                "execution finished successfuly {:?}",
                call_info.execution.retdata
            );

            #[cfg(feature = "state_dump")]
            {
                state_dump::create_call_state_dump(&mut state, &tx, &call_info).unwrap();
            }
        }
    }
}

fn parse_network(network: &str) -> ChainId {
    match network.to_lowercase().as_str() {
        "mainnet" => ChainId::Mainnet,
        "testnet" => ChainId::Sepolia,
        _ => panic!("Invalid network name, it should be one of: mainnet, testnet"),
    }
}

fn build_cached_state(network: &str, block_number: u64) -> CachedState<RpcCachedStateReader> {
    let rpc_reader = build_reader(network, block_number);
    CachedState::new(rpc_reader)
}
fn build_reader(network: &str, block_number: u64) -> RpcCachedStateReader {
    let block_number = BlockNumber(block_number);
    let rpc_chain = parse_network(network);

    RpcCachedStateReader::new(RpcStateReader::new(rpc_chain, block_number))
}

fn show_execution_data(
    state: &mut CachedState<impl StateReader>,
    reader: &impl StateReader,
    tx_hash_str: String,
    chain_str: &str,
    block_number: u64,
    charge_fee: bool,
) {
    let _transaction_execution_span = info_span!(
        "transaction",
        hash = tx_hash_str,
        chain = chain_str,
        block = block_number
    )
    .entered();
    info!("starting execution");

    let tx_hash = TransactionHash(felt!(tx_hash_str.as_str()));
    let flags = ExecutionFlags {
        only_query: false,
        charge_fee,
        validate: true,
    };

    let (tx, context) = match fetch_transaction_with_state(reader, &tx_hash, flags) {
        Ok(x) => x,
        Err(err) => {
            return error!("failed to fetch transaction: {err}");
        }
    };

    let execution_info_result = tx.execute(state, &context);

    #[cfg(feature = "state_dump")]
    state_dump::create_state_dump(state, block_number, &tx_hash_str, &execution_info_result);

    let execution_info = match execution_info_result {
        Ok(x) => x,
        Err(err) => {
            error!("execution failed: {}", err);
            return;
        }
    };

    match reader.get_transaction_receipt(&tx_hash) {
        Ok(rpc_receipt) => {
            compare_execution(execution_info, rpc_receipt);
        }
        Err(_) => {
            error!("failed to get transaction receipt, could not compare to rpc");
        }
    };
}

fn compare_execution(
    execution: TransactionExecutionInfo,
    rpc_receipt: RpcTransactionReceipt,
) -> bool {
    let reverted = execution.is_reverted();
    let rpc_reverted = matches!(
        rpc_receipt.execution_status,
        TransactionExecutionStatus::Reverted(_)
    );

    let status_matches = reverted == rpc_reverted;

    let da_gas = &execution.receipt.da_gas;
    let da_gas_str = format!(
        "{{ l1_da_gas: {}, l1_gas: {} }}",
        da_gas.l1_data_gas, da_gas.l1_gas
    );

    let exec_rsc = &execution.receipt.resources.starknet_resources;

    let events_and_msgs = format!(
        "{{ events_number: {}, l2_to_l1_messages_number: {} }}",
        exec_rsc.archival_data.event_summary.n_events + 1,
        exec_rsc.messages.l2_to_l1_payload_lengths.len(),
    );
    let rpc_events_and_msgs = format!(
        "{{ events_number: {}, l2_to_l1_messages_number: {} }}",
        rpc_receipt.events.len(),
        rpc_receipt.messages_sent.len(),
    );

    // currently adding 1 because the sequencer is counting only the
    // events produced by the inner calls of a callinfo
    let events_match =
        exec_rsc.archival_data.event_summary.n_events + 1 == rpc_receipt.events.len();
    let msgs_match =
        rpc_receipt.messages_sent.len() == exec_rsc.messages.l2_to_l1_payload_lengths.len();

    let events_msgs_match = events_match && msgs_match;

    let state_changes = exec_rsc.state.state_changes_for_fee;
    let state_changes_for_fee_str = format!(
        "{{ n_class_hash_updates: {}, n_compiled_class_hash_updates: {}, n_modified_contracts: {}, n_storage_updates: {} }}",
        state_changes.state_changes_count.n_class_hash_updates,
        state_changes.state_changes_count.n_compiled_class_hash_updates,
        state_changes.state_changes_count.n_modified_contracts,
        state_changes.state_changes_count.n_storage_updates
    );

    let execution_gas = execution.receipt.fee;
    let rpc_gas = rpc_receipt.actual_fee;
    debug!(?execution_gas, ?rpc_gas, "execution actual fee");

    let revert_error = execution.revert_error.map(|err| match err {
        RevertError::Execution(e) => e.to_string(),
        RevertError::PostExecution(p) => p.to_string(),
    });

    if !status_matches || !events_msgs_match {
        let root_of_error = if !status_matches {
            "EXECUTION STATUS DIVERGED"
        } else if !(events_match || msgs_match) {
            "MESSAGE AND EVENT COUNT DIVERGED"
        } else if !events_match {
            "EVENT COUNT DIVERGED"
        } else {
            "MESSAGE COUNT DIVERGED"
        };

        error!(
            reverted,
            rpc_reverted,
            root_of_error = root_of_error,
            execution_error_message = revert_error,
            n_events_and_messages = events_and_msgs,
            rpc_n_events_and_msgs = rpc_events_and_msgs,
            da_gas = da_gas_str,
            state_changes_for_fee_str,
            "rpc and execution status diverged"
        );

        false
    } else {
        info!(
            reverted,
            rpc_reverted,
            execution_error_message = revert_error,
            n_events_and_messages = events_and_msgs,
            rpc_n_events_and_msgs = rpc_events_and_msgs,
            da_gas = da_gas_str,
            state_changes_for_fee_str,
            "execution finished successfully"
        );

        true
    }
}

fn set_global_subscriber() {
    #[cfg(not(feature = "structured_logging"))]
    let default_env_filter = EnvFilter::try_new("replay=info,rpc_state_reader=info")
        .expect("hard-coded env filter should be valid");

    #[cfg(feature = "structured_logging")]
    let default_env_filter =
        EnvFilter::try_new("replay=info,blockifier=info,rpc_state_reader=info,cairo_native=trace")
            .expect("hard-coded env filter should be valid");

    let env_filter = EnvFilter::try_from_default_env().unwrap_or(default_env_filter);

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_file(false)
        .with_line_number(false);

    #[cfg(not(feature = "structured_logging"))]
    let subscriber = subscriber.pretty();
    #[cfg(feature = "structured_logging")]
    let subscriber = subscriber.json();

    subscriber.finish().init();
}
