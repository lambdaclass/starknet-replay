use blockifier::execution::contract_class::TrackedResource;
use blockifier::execution::entry_point::{
    EntryPointExecutionContext, EntryPointRevertInfo, ExecutableCallEntryPoint,
    SierraGasRevertTracker,
};
use blockifier::execution::execution_utils::execute_entry_point_call;
use blockifier::state::cached_state::CachedState;
use blockifier::state::state_api::StateReader as _;
use blockifier::transaction::account_transaction::ExecutionFlags;
use clap::{Parser, Subcommand};

use execution::{execute_block, execute_txs};
use rpc_state_reader::cache::RpcCachedStateReader;
use rpc_state_reader::execution::fetch_transaction_with_state;
use rpc_state_reader::reader::RpcStateReader;
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_api::execution_resources::GasAmount;

use starknet_api::felt;
use starknet_api::transaction::TransactionHash;
use state_reader::full_state_reader::FullStateReader;
use state_reader::remote_state_reader::{url_from_env, RemoteStateReader};
use tracing::{error, info};
use tracing_subscriber::{util::SubscriberInitExt, EnvFilter};

#[cfg(feature = "benchmark")]
use crate::benchmark::aggregate_executions;
#[cfg(feature = "block-composition")]
use {
    block_composition::save_entry_point_execution, chrono::DateTime,
    rpc_state_reader::execution::fetch_block_context,
};

use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(feature = "profiling")]
use {std::thread, std::time::Duration};

mod execution;

#[cfg(feature = "benchmark")]
mod benchmark;
#[cfg(feature = "block-composition")]
mod block_composition;
#[cfg(feature = "state_dump")]
mod state_dump;

#[cfg(feature = "with-libfunc-profiling")]
mod libfunc_profile;

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
            let chain = parse_network(&chain);
            let url = url_from_env(chain);
            let block_number = BlockNumber(block_number);
            let tx_hash = TransactionHash(felt!(tx_hash.as_str()));

            let remote_reader = RemoteStateReader::new(url);
            let full_reader = FullStateReader::new(remote_reader);

            let execution_flags = ExecutionFlags {
                only_query: false,
                charge_fee,
                validate: true,
            };

            execute_txs(&full_reader, block_number, vec![tx_hash], execution_flags)
                .expect("failed to execute transaction");
        }
        ReplayExecute::Block {
            block_number,
            chain,
            charge_fee,
        } => {
            let chain = parse_network(&chain);
            let url = url_from_env(chain);
            let block_number = BlockNumber(block_number);

            let remote_reader = RemoteStateReader::new(url);
            let full_reader = FullStateReader::new(remote_reader);

            let execution_flags = ExecutionFlags {
                only_query: false,
                charge_fee,
                validate: true,
            };
            execute_block(&full_reader, block_number, execution_flags)
                .expect("failed to execute block");
        }
        ReplayExecute::BlockTxs {
            chain,
            block_number,
            txs,
            charge_fee,
        } => {
            let tx_hashes: Vec<TransactionHash> = txs
                .into_iter()
                .map(|hash| TransactionHash(felt!(hash.as_str())))
                .collect();

            let chain = parse_network(&chain);
            let url = url_from_env(chain);
            let block_number = BlockNumber(block_number);

            let remote_reader = RemoteStateReader::new(url);
            let full_reader = FullStateReader::new(remote_reader);

            let execution_flags = ExecutionFlags {
                only_query: false,
                charge_fee,
                validate: true,
            };
            execute_txs(&full_reader, block_number, tx_hashes, execution_flags)
                .expect("failed to execute block");
        }
        ReplayExecute::BlockRange {
            block_start,
            block_end,
            chain,
            charge_fee,
        } => {
            let chain = parse_network(&chain);
            let url = url_from_env(chain);

            let remote_reader = RemoteStateReader::new(url);
            let full_reader = FullStateReader::new(remote_reader);

            let execution_flags = ExecutionFlags {
                only_query: false,
                charge_fee,
                validate: true,
            };

            for block_number in block_start..=block_end {
                execute_block(
                    &full_reader,
                    BlockNumber(block_number),
                    execution_flags.clone(),
                )
                .expect("failed to execute block");
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
            let chain = parse_network(&chain);
            let url = url_from_env(chain);

            let remote_reader = RemoteStateReader::new(url);
            let full_reader = FullStateReader::new(remote_reader);

            let execution_flags = ExecutionFlags {
                only_query: false,
                charge_fee: true,
                validate: true,
            };

            // We execute the block range once, to ensure that everything is cached.
            for block_number in block_start..=block_end {
                execute_block(
                    &full_reader,
                    BlockNumber(block_number),
                    execution_flags.clone(),
                )
                .expect("failed to execute block");
            }

            // We pause the main thread to differentiate
            // caching from benchmarking from within a profiler
            #[cfg(feature = "profiling")]
            thread::sleep(Duration::from_secs(1));

            let mut block_executions = Vec::new();

            for _ in 0..number_of_runs {
                for block_number in block_start..=block_end {
                    let executions = execute_block(
                        &full_reader,
                        BlockNumber(block_number),
                        execution_flags.clone(),
                    )
                    .expect("failed to execute block");

                    // TODO: The `execute_block` output representation
                    // is translated into the representation used by
                    // `aggregate_executions`. We should update the benchmark
                    // feature to better use the representation returned on
                    // execution, instead of translating them.
                    block_executions.push((
                        BlockNumber(block_number),
                        executions
                            .into_iter()
                            .filter_map(|x| Some((x.hash, x.result.ok()?, x.time)))
                            .collect(),
                    ));
                }
            }

            let benchmarking_data = aggregate_executions(block_executions);

            let file = std::fs::File::create(output).unwrap();
            serde_json::to_writer_pretty(file, &benchmarking_data).unwrap();
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
            let url = url_from_env(chain);
            let block_number = BlockNumber(block);
            let tx_hash = TransactionHash(felt!(tx.as_str()));

            let remote_reader = RemoteStateReader::new(url);
            let full_reader = FullStateReader::new(remote_reader);

            let execution_flags = ExecutionFlags {
                only_query: false,
                charge_fee: true,
                validate: true,
            };

            // We execute the transaction once, to ensure that everything is cached.
            execute_txs(
                &full_reader,
                block_number,
                vec![tx_hash],
                execution_flags.clone(),
            )
            .expect("failed to execute transaction");

            // We pause the main thread to differentiate
            // caching from benchmarking from within a profiler
            #[cfg(feature = "profiling")]
            thread::sleep(Duration::from_secs(1));

            let mut block_executions = Vec::new();

            for _ in 0..number_of_runs {
                // We execute the transaction once, to ensure that everything is cached.
                let executions = execute_txs(
                    &full_reader,
                    block_number,
                    vec![tx_hash],
                    execution_flags.clone(),
                )
                .expect("failed to execute transaction");

                // TODO: The `execute_block` output representation
                // is translated into the representation used by
                // `aggregate_executions`. We should update the benchmark
                // feature to better use the representation returned on
                // execution, instead of translating them.
                block_executions.push((
                    block_number,
                    executions
                        .into_iter()
                        .filter_map(|x| Some((x.hash, x.result.ok()?, x.time)))
                        .collect(),
                ));
            }

            let benchmarking_data = aggregate_executions(block_executions);

            let file = std::fs::File::create(output).unwrap();
            serde_json::to_writer_pretty(file, &benchmarking_data).unwrap();
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
                let _block_span = tracing::info_span!("block", number = block_number).entered();

                let mut state = build_cached_state(&chain, block_number - 1);
                let reader = build_reader(&chain, block_number);

                let flags = ExecutionFlags {
                    only_query: false,
                    charge_fee: false,
                    validate: true,
                };

                let block_context = fetch_block_context(&reader).unwrap();

                // fetch and execute transactions
                let entrypoints =
                    rpc_state_reader::reader::StateReader::get_block_with_tx_hashes(&reader)
                        .unwrap()
                        .transactions
                        .into_iter()
                        .map(|hash| {
                            let (tx, _) =
                                fetch_transaction_with_state(&reader, &hash, flags.clone())
                                    .unwrap();
                            let execution =
                        blockifier::transaction::transactions::ExecutableTransaction::execute(
                            &tx,
                            &mut state,
                            &block_context,
                        );
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
