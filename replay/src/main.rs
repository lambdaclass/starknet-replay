use blockifier::state::cached_state::CachedState;
use blockifier::state::errors::StateError;
use blockifier::transaction::objects::TransactionExecutionInfo;
use clap::{Parser, Subcommand};

use rpc_state_reader::execution::execute_tx_configurable;
use rpc_state_reader::objects::RpcTransactionReceipt;
use rpc_state_reader::reader::{RpcChain, RpcStateReader};
use starknet_api::block::BlockNumber;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::{TransactionExecutionStatus, TransactionHash};
use tracing::{debug, error, info, info_span};
use tracing_subscriber::{util::SubscriberInitExt, EnvFilter};

#[cfg(feature = "benchmark")]
use {
    crate::benchmark::{execute_block_range, fetch_block_range_data, fetch_transaction_data},
    std::{ops::Div, time::Instant},
};

#[cfg(feature = "benchmark")]
mod benchmark;
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
    },
    #[cfg(feature = "benchmark")]
    #[clap(about = "Measures the time it takes to run a list of transactions.
        Caches all rpc data before the benchmark runs to provide accurate results.
        It only works if the transaction doesn't depend on another transaction in the same block")]
    BenchMultiTx
}

fn main() {
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
            show_execution_data(&mut state, tx_hash, &chain, block_number, charge_fee);
        }
        ReplayExecute::Block {
            block_number,
            chain,
            charge_fee,
        } => {
            let _block_span = info_span!("block", number = block_number).entered();

            let mut state = build_cached_state(&chain, block_number - 1);

            let transaction_hashes = get_transaction_hashes(&chain, block_number)
                .expect("Unable to fetch the transaction hashes.");
            for tx_hash in transaction_hashes {
                show_execution_data(
                    &mut state,
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

                let transaction_hashes = get_transaction_hashes(&chain, block_number)
                    .expect("Unable to fetch the transaction hashes.");

                for tx_hash in transaction_hashes {
                    show_execution_data(
                        &mut state,
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

            {
                let _benchmark_span = info_span!("benchmarking block range").entered();
                let before_execution = Instant::now();

                for _ in 0..number_of_runs {
                    execute_block_range(&mut block_range_data);
                }

                let execution_time = before_execution.elapsed();
                let total_run_time = execution_time.as_secs_f64();
                let average_run_time = total_run_time.div(number_of_runs as f64);
                info!(
                    block_start = block_start.0,
                    block_end = block_end.0,
                    number_of_runs,
                    total_run_time,
                    average_run_time,
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

            {
                let _benchmark_span = info_span!("benchmarking transaction").entered();
                let before_execution = Instant::now();

                for _ in 0..number_of_runs {
                    execute_block_range(&mut block_range_data);
                }

                let execution_time = before_execution.elapsed();
                let total_run_time = execution_time.as_secs_f64();
                let average_run_time = total_run_time.div(number_of_runs as f64);
                info!(
                    tx = tx,
                    block = block.0,
                    number_of_runs,
                    total_run_time,
                    average_run_time,
                    "benchmark finished",
                );
            }
        }
        #[cfg(feature = "benchmark")]
        ReplayExecute::BenchMultiTx => {
            let blocks = vec![
                803245,
                 803233,
                 803223,
                 803215,
                 803198,
                 803194,
                 803019,
                 802967,
                 802964,
                 802964,
                802962,
                 802962,
                802907,
                802903,
            ];
            let txs = vec![
                "0x7f2f1e76fe3d58cf20cc987cda37327206b711977a25955ed818a7b32378c0",
                "0x69c279695d338e93bf18151259612016b298b28fbf33412e7ebde9e06bed2e0",
                "0x1760352413f6a648fad6ee315285f1ed14139248279caaae9f5929545cb5cdb",
                "0x7ca4ddbd5e1cfd269092490cbe18c2794b7918e9ba64dfc386f483814333370",
                "0x3fbce25696fd0ecd0b265127f174f990f73f42e6fb7bbc4eb7f952ed40aaf20",
                "0x2634ddda80c52649556725ac3b700b543165bac7fb28575f722662425292779",
                "0x692d00de6b2e0b03deaee822c4a696ee1b68b3c4f978be1fd26cbbc09cec09d",
                "0x60ebb7ebdb4ec104367341e6c7447db3510cefa36084297ce0abe7fe86954c1",
                "0x2c4aa75cd709100e93437b0a2157f6262b1c2389d3fb4cf0dcdaa11155f8e65",
                "0x52ea7a6e762cb7746b8c67ac75a76ab9e5258833d90becd50001b8154f2ba51",
                "0xfe77326cffee6415680cb1eb413b34a067fadb54190731f382762232d2fd51",
                "0x63d26dd5d7c2b9b8633541267664e2bf26577985469fbf47b9c7605d5513d2a",
                "0x8d586ca36d2737aee12bbb24eaf4874dd0a500b7a0b74d42c457af383358f9",
                "0x713aeaf5914892f486b33ae6ccb5359c76d74c059b1426230adb262bde3522",
            ];
            let number_of_runs = 100;
            let chain = parse_network("mainnet");
            let tx_block = txs.iter().zip(blocks);
            let mut block_range_data = {
                let _caching_span = info_span!("caching block range").entered();
                let mut block_range_data = vec![];

                info!("fetching transaction data");
                for (tx, block) in tx_block {
                    let block = BlockNumber(block);
                    let transaction_data = fetch_transaction_data(&tx, block, chain);
                    block_range_data.push(transaction_data);
                }

                info!("filling up execution cache");
                execute_block_range(&mut block_range_data);

                for (cached_state, ..) in &mut block_range_data {
                    cached_state.state.disable();
                }

                block_range_data
            };

            {
                let _benchmark_span = info_span!("benchmarking transaction").entered();
                let before_execution = Instant::now();

                for _ in 0..number_of_runs {
                    execute_block_range(&mut block_range_data);
                }

                let execution_time = before_execution.elapsed();
                let total_run_time = execution_time.as_secs_f64();
                let average_run_time = total_run_time.div(number_of_runs as f64);

                info!(
                    number_of_runs,
                    total_run_time,
                    average_run_time,
                    "benchmark finished",
                );
            }
        }
    }
}

fn parse_network(network: &str) -> RpcChain {
    match network.to_lowercase().as_str() {
        "mainnet" => RpcChain::MainNet,
        "testnet" => RpcChain::TestNet,
        "testnet2" => RpcChain::TestNet2,
        _ => panic!("Invalid network name, it should be one of: mainnet, testnet, testnet2"),
    }
}

fn build_cached_state(network: &str, block_number: u64) -> CachedState<RpcStateReader> {
    let previous_block_number = BlockNumber(block_number);
    let rpc_chain = parse_network(network);
    let rpc_reader = RpcStateReader::new(rpc_chain, previous_block_number);

    CachedState::new(rpc_reader)
}

fn show_execution_data(
    state: &mut CachedState<RpcStateReader>,
    tx_hash: String,
    chain: &str,
    block_number: u64,
    charge_fee: bool,
) {
    let _transaction_execution_span = info_span!("transaction", hash = tx_hash, chain).entered();

    info!("starting execution");

    let previous_block_number = BlockNumber(block_number - 1);

    let execution_info = match execute_tx_configurable(
        state,
        &tx_hash,
        previous_block_number,
        false,
        true,
        charge_fee,
    ) {
        Ok(x) => x,
        Err(error_reason) => {
            error!("execution failed unexpectedly: {}", error_reason);
            return;
        }
    };

    #[cfg(feature = "state_dump")]
    {
        use std::path::Path;
        #[cfg(feature = "only_cairo_vm")]
        let root = Path::new("state_dumps/vm");
        #[cfg(not(feature = "only_cairo_vm"))]
        let root = Path::new("state_dumps/native");
        let root = root.join(format!("block{}", block_number));

        let mut path = root.join(&tx_hash);
        path.set_extension("json");

        state_dump::dump_state_diff(state, &execution_info, &path).unwrap();
    }

    let transaction_hash = TransactionHash(StarkHash::from_hex(&tx_hash).unwrap());
    match state.state.get_transaction_receipt(&transaction_hash) {
        Ok(rpc_receipt) => {
            compare_execution(execution_info, rpc_receipt);
        }
        Err(_) => {
            error!(
                transaction_hash = tx_hash,
                chain = chain,
                "failed to get transaction receipt, could not compare to rpc"
            );
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
        exec_rsc.n_events + 1,
        exec_rsc.message_cost_info.l2_to_l1_payload_lengths.len(),
    );
    let rpc_events_and_msgs = format!(
        "{{ events_number: {}, l2_to_l1_messages_number: {} }}",
        rpc_receipt.events.len(),
        rpc_receipt.messages_sent.len(),
    );

    // currently adding 1 because the sequencer is counting only the
    // events produced by the inner calls of a callinfo
    let events_match = exec_rsc.n_events + 1 == rpc_receipt.events.len();
    let msgs_match = rpc_receipt.messages_sent.len()
        == exec_rsc.message_cost_info.l2_to_l1_payload_lengths.len();

    let events_msgs_match = events_match && msgs_match;

    let state_changes = exec_rsc.state_changes_for_fee;
    let state_changes_for_fee_str = format!(
        "{{ n_class_hash_updates: {}, n_compiled_class_hash_updates: {}, n_modified_contracts: {}, n_storage_updates: {} }}",
        state_changes.n_class_hash_updates,
        state_changes.n_compiled_class_hash_updates,
        state_changes.n_modified_contracts,
        state_changes.n_storage_updates
    );

    let execution_gas = execution.receipt.fee;
    let rpc_gas = rpc_receipt.actual_fee;
    debug!(?execution_gas, ?rpc_gas, "execution actual fee");

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
            execution_error_message = execution.revert_error,
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
            execution_error_message = execution.revert_error,
            n_events_and_messages = events_and_msgs,
            rpc_n_events_and_msgs = rpc_events_and_msgs,
            da_gas = da_gas_str,
            state_changes_for_fee_str,
            "execution finished successfully"
        );

        true
    }
}

fn get_transaction_hashes(
    network: &str,
    block_number: u64,
) -> Result<Vec<TransactionHash>, StateError> {
    let network = parse_network(network);
    let block_value = BlockNumber(block_number);
    let rpc_state = RpcStateReader::new(network, block_value);
    Ok(rpc_state.get_block_with_tx_hashes()?.transactions)
}

fn set_global_subscriber() {
    #[cfg(not(feature = "structured_logging"))]
    let default_env_filter =
        EnvFilter::try_new("replay=info").expect("hard-coded env filter should be valid");

    #[cfg(feature = "structured_logging")]
    let default_env_filter =
        EnvFilter::try_new("replay=info,blockifier=info,rpc_state_reader=info,cairo_native=info")
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
