use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

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
use execution::{execute_block, execute_txs, get_block_context, get_blockifier_transaction};
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_api::execution_resources::GasAmount;
use starknet_api::felt;
use starknet_api::transaction::TransactionHash;
use state_reader::block_state_reader::BlockStateReader;
use state_reader::full_state_reader::FullStateReader;
use tracing::{error, info};
use tracing_subscriber::{util::SubscriberInitExt, EnvFilter};

#[cfg(feature = "block-composition")]
use {block_composition::save_entry_point_execution, chrono::DateTime};

#[cfg(feature = "benchmark")]
use {
    crate::benchmark::benchmark_compilation,
    cairo_lang_starknet_classes::contract_class::ContractClass,
    starknet_api::{core::ClassHash, hash::StarkHash},
    starknet_core::types::ContractClass as ProcessedContractClass,
    starknet_core::types::{BlockId, BlockTag},
    state_reader::class_manager::processed_class_to_contract_class,
    std::collections::HashMap,
    std::io::{BufRead, BufReader},
};

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
    #[cfg(feature = "benchmark")]
    /// Benchmarks the compilation of contract classes
    BenchCompilation {
        /// Path to read input classes from.
        ///
        /// Each line contains two whitespace separated values:
        /// - Network, either mainnet or testnet;
        /// - Class Hash, in hexadecimal form.
        #[clap(verbatim_doc_comment)]
        input: PathBuf,
        /// Output file name for benchmark summary.
        #[clap(long)]
        output: Option<PathBuf>,
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
            let block_number = BlockNumber(block_number);
            let tx_hash = TransactionHash(felt!(tx_hash.as_str()));

            let full_reader = FullStateReader::new(chain);

            let execution_flags = ExecutionFlags {
                only_query: false,
                charge_fee,
                validate: true,
                strict_nonce_check: false,
            };

            execute_txs(&full_reader, block_number, vec![tx_hash], execution_flags)
                .expect("failed to execute transaction");
            log_cache_statistics(&full_reader);
        }
        ReplayExecute::Block {
            block_number,
            chain,
            charge_fee,
        } => {
            let chain = parse_network(&chain);
            let block_number = BlockNumber(block_number);

            let full_reader = FullStateReader::new(chain);

            let execution_flags = ExecutionFlags {
                only_query: false,
                charge_fee,
                validate: true,
                strict_nonce_check: false,
            };
            execute_block(&full_reader, block_number, execution_flags)
                .expect("failed to execute block");
            log_cache_statistics(&full_reader);
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
            let block_number = BlockNumber(block_number);

            let full_reader = FullStateReader::new(chain);

            let execution_flags = ExecutionFlags {
                only_query: false,
                charge_fee,
                validate: true,
                strict_nonce_check: false,
            };
            execute_txs(&full_reader, block_number, tx_hashes, execution_flags)
                .expect("failed to execute block");
            log_cache_statistics(&full_reader);
        }
        ReplayExecute::BlockRange {
            block_start,
            block_end,
            chain,
            charge_fee,
        } => {
            let chain = parse_network(&chain);
            let full_reader = FullStateReader::new(chain);

            let execution_flags = ExecutionFlags {
                only_query: false,
                charge_fee,
                validate: true,
                strict_nonce_check: false,
            };

            for block_number in block_start..=block_end {
                execute_block(
                    &full_reader,
                    BlockNumber(block_number),
                    execution_flags.clone(),
                )
                .expect("failed to execute block");
            }
            log_cache_statistics(&full_reader);
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
            let full_reader = FullStateReader::new(chain);

            let execution_flags = ExecutionFlags {
                only_query: false,
                charge_fee: true,
                validate: true,
                strict_nonce_check: false,
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

            log_cache_statistics(&full_reader);
            full_reader.reset_counters();

            // We pause the main thread to differentiate
            // caching from benchmarking from within a profiler
            #[cfg(feature = "profiling")]
            thread::sleep(Duration::from_secs(1));

            let mut executions = Vec::new();

            for _ in 0..number_of_runs {
                for block_number in block_start..=block_end {
                    let mut block_executions = execute_block(
                        &full_reader,
                        BlockNumber(block_number),
                        execution_flags.clone(),
                    )
                    .expect("failed to execute block");

                    assert_eq!(
                        full_reader.get_miss_counter(),
                        0,
                        "cache miss during a benchmark"
                    );

                    executions.append(&mut block_executions);
                }
            }

            let benchmarking_data = benchmark::BenchData::aggregate(&executions);

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
            let block_number = BlockNumber(block);
            let tx_hash = TransactionHash(felt!(tx.as_str()));

            let full_reader = FullStateReader::new(chain);

            let execution_flags = ExecutionFlags {
                only_query: false,
                charge_fee: true,
                validate: true,
                strict_nonce_check: false,
            };

            // We execute the transaction once, to ensure that everything is cached.
            execute_txs(
                &full_reader,
                block_number,
                vec![tx_hash],
                execution_flags.clone(),
            )
            .expect("failed to execute transaction");

            log_cache_statistics(&full_reader);
            full_reader.reset_counters();

            // We pause the main thread to differentiate
            // caching from benchmarking from within a profiler
            #[cfg(feature = "profiling")]
            thread::sleep(Duration::from_secs(1));

            let mut executions = Vec::new();

            for _ in 0..number_of_runs {
                let mut block_executions = execute_txs(
                    &full_reader,
                    block_number,
                    vec![tx_hash],
                    execution_flags.clone(),
                )
                .expect("failed to execute transaction");

                assert_eq!(
                    full_reader.get_miss_counter(),
                    0,
                    "cache miss during a benchmark"
                );

                executions.append(&mut block_executions);
            }

            let benchmarking_data = benchmark::BenchData::aggregate(&executions);

            let file = std::fs::File::create(output).unwrap();
            serde_json::to_writer_pretty(file, &benchmarking_data).unwrap();
        }
        #[cfg(feature = "benchmark")]
        ReplayExecute::BenchCompilation { input, output } => {
            let class_hashes = read_class_hashes_to_compile(input);
            let classes = fetch_classes_to_compile(class_hashes);

            let mut benchmark = Vec::new();
            for (class_hash, contract_class) in classes {
                benchmark.push(
                    benchmark_compilation(class_hash, contract_class)
                        .expect("failed to compile class"),
                );
            }

            if let Some(output) = output {
                let mut writer = csv::Writer::from_path(output).expect("failed to create writer");
                for tx_data in &benchmark {
                    writer
                        .serialize(tx_data)
                        .expect("failed to serialize benchmark data");
                }
            }
        }
        #[cfg(feature = "block-composition")]
        ReplayExecute::BlockCompose {
            block_start,
            block_end,
            chain,
        } => {
            let chain = parse_network(&chain);
            let full_reader = FullStateReader::new(chain.clone());

            let execution_flags = ExecutionFlags {
                only_query: false,
                charge_fee: true,
                validate: true,
                strict_nonce_check: false,
            };

            let mut block_executions = Vec::new();

            for block_number in block_start..=block_end {
                let block_context =
                    execution::get_block_context(&full_reader, BlockNumber(block_number))
                        .expect("failed to fetch block context");

                let block_timestamp = DateTime::from_timestamp(
                    block_context.block_info().block_timestamp.0 as i64,
                    0,
                )
                .expect("failed to build timestamp")
                .to_string();

                let executions = execute_block(
                    &full_reader,
                    BlockNumber(block_number),
                    execution_flags.clone(),
                )
                .expect("failed to execute block");

                let entrypoints = executions.into_iter().map(|tx| tx.result).collect();

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
            let chain = parse_network(&chain);
            let block_number = BlockNumber(block_number);
            let tx_hash = TransactionHash(felt!(tx.as_str()));

            let full_reader = FullStateReader::new(chain);

            let block_reader = BlockStateReader::new(
                block_number
                    .prev()
                    .expect("block number should not be zero"),
                &full_reader,
            );
            let mut state = CachedState::new(block_reader);

            let execution_flags = ExecutionFlags {
                only_query: false,
                charge_fee: true,
                validate: true,
                strict_nonce_check: false,
            };

            let call_file = File::open(call_path).unwrap();
            let call: ExecutableCallEntryPoint = serde_json::from_reader(call_file).unwrap();

            // We fetch the compile class from the next block, instead of the current block.
            // This ensures that it always exists.
            let compiled_class = full_reader
                .get_compiled_class(block_number, call.class_hash)
                .expect("failed to target compiled class");

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
                let block_context = get_block_context(&full_reader, block_number)
                    .expect("failed to get block context");

                let tx = get_blockifier_transaction(
                    &full_reader,
                    block_number,
                    tx_hash,
                    execution_flags,
                )
                .expect("failed to get executable transaction");

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

#[cfg(feature = "benchmark")]
fn fetch_classes_to_compile(
    class_hashes: Vec<(ChainId, ClassHash)>,
) -> Vec<(ClassHash, ContractClass)> {
    let mut classes = Vec::new();
    let mut state_readers = HashMap::new();
    for (chain_id, class_hash) in class_hashes {
        let state_reader = state_readers
            .entry(chain_id.clone())
            .or_insert_with(|| FullStateReader::new(chain_id.clone()));

        let contract_class = state_reader
            .get_contract_class(BlockId::Tag(BlockTag::Latest), class_hash)
            .expect("failed to get contract class");

        let ProcessedContractClass::Sierra(sierra_class) = contract_class else {
            panic!("cannot compile a deprecated contract class")
        };

        let contract_class =
            processed_class_to_contract_class(&sierra_class).expect("failed to get contract class");

        classes.push((class_hash, contract_class));
    }
    classes
}

#[cfg(feature = "benchmark")]
fn read_class_hashes_to_compile(path: PathBuf) -> Vec<(ChainId, ClassHash)> {
    let mut class_hashes = Vec::new();
    let input_reader = BufReader::new(File::open(path).expect("failed to open file"));
    for line in input_reader.lines() {
        let [network_str, class_hash_str] = line
            .as_ref()
            .expect("failed to read line")
            .split_whitespace()
            .collect::<Vec<_>>()
            .try_into()
            .expect("expected 2 arguments per line");

        let class_hash =
            ClassHash(StarkHash::from_hex(class_hash_str).expect("failed to parse class hash"));
        let network = parse_network(network_str);

        class_hashes.push((network, class_hash));
    }
    class_hashes
}

fn parse_network(network: &str) -> ChainId {
    match network.to_lowercase().as_str() {
        "mainnet" => ChainId::Mainnet,
        "testnet" => ChainId::Sepolia,
        _ => panic!("Invalid network name, it should be one of: mainnet, testnet"),
    }
}

fn log_cache_statistics(full_state_reader: &FullStateReader) {
    info!(
        hits = full_state_reader.get_hit_counter(),
        miss = full_state_reader.get_miss_counter(),
        timeout_retries = full_state_reader.get_rpc_timeout_counter(),
        "cache statistics"
    )
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
