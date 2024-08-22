use std::str::FromStr;

use clap::{Parser, Subcommand};
use rpc_state_reader::blockifier::state::cached_state::CachedState;
use rpc_state_reader::{
    blockifier_state_reader::RpcStateReader,
    rpc_state::{BlockValue, RpcChain, RpcState},
    rpc_state_errors::RpcStateError,
};

use rpc_state_reader::blockifier_state_reader::execute_tx_configurable;
use starknet_api::block::BlockNumber;
use tracing::{debug, error, info, info_span};
use tracing_subscriber::filter::Directive;
use tracing_subscriber::{util::SubscriberInitExt, EnvFilter};

#[cfg(feature = "benchmark")]
use {
    crate::benchmark::{execute_block_range, fetch_block_range_data},
    std::{ops::Div, time::Instant},
};

#[cfg(feature = "benchmark")]
mod benchmark;

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
    },
    #[clap(about = "Execute all the transactions in a given block.")]
    Block { chain: String, block_number: u64 },
    #[clap(about = "Execute all the transactions in a given range of blocks.")]
    BlockRange {
        block_start: u64,
        block_end: u64,
        chain: String,
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
}

fn main() {
    set_global_subscriber();

    let cli = ReplayCLI::parse();

    if cfg!(feature = "use-sierra-emu") {
        info!("Using the sierra-emu blockifier");
    } else {
        info!("Using the cairo native blockifier");
    }

    match cli.subcommand {
        ReplayExecute::Tx {
            tx_hash,
            chain,
            block_number,
        } => {
            let mut state = build_cached_state(&chain, block_number - 1);
            show_execution_data(&mut state, tx_hash, &chain, block_number);
        }
        ReplayExecute::Block {
            block_number,
            chain,
        } => {
            let _block_span = info_span!("block", number = block_number).entered();

            let mut state = build_cached_state(&chain, block_number - 1);

            let transaction_hashes = get_transaction_hashes(&chain, block_number)
                .expect("Unable to fetch the transaction hashes.");
            for tx_hash in transaction_hashes {
                show_execution_data(&mut state, tx_hash, &chain, block_number);
            }
        }
        ReplayExecute::BlockRange {
            block_start,
            block_end,
            chain,
        } => {
            info!("executing block range: {} - {}", block_start, block_end);

            for block_number in block_start..=block_end {
                let _block_span = info_span!("block", number = block_number).entered();

                let mut state = build_cached_state(&chain, block_number - 1);

                let transaction_hashes = get_transaction_hashes(&chain, block_number)
                    .expect("Unable to fetch the transaction hashes.");

                for tx_hash in transaction_hashes {
                    show_execution_data(&mut state, tx_hash, &chain, block_number);
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
    let rpc_reader = RpcStateReader(
        RpcState::new_rpc(rpc_chain, previous_block_number.into())
            .expect("failed to create state reader"),
    );

    CachedState::new(rpc_reader)
}

fn show_execution_data(
    state: &mut CachedState<RpcStateReader>,
    tx_hash: String,
    chain: &str,
    block_number: u64,
) {
    let _transaction_execution_span = info_span!("transaction", hash = tx_hash, chain).entered();

    info!("starting execution");

    let previous_block_number = BlockNumber(block_number - 1);

    let (execution_info, _trace, rpc_receipt) =
        match execute_tx_configurable(state, &tx_hash, previous_block_number, false, true) {
            Ok(x) => x,
            Err(error_reason) => {
                error!("execution failed unexpectedly: {}", error_reason);
                return;
            }
        };

    let execution_status = match &execution_info.revert_error {
        Some(_) => "REVERTED",
        None => "SUCCEEDED",
    };
    let rpc_execution_status = rpc_receipt.execution_status;
    let status_matches = execution_status == rpc_execution_status;

    if !status_matches {
        error!(
            transaction_hash = tx_hash,
            chain = chain,
            execution_status,
            rpc_execution_status,
            execution_error_message = execution_info.revert_error,
            "rpc and execution status diverged"
        )
    } else {
        info!(
            transaction_hash = tx_hash,
            chain = chain,
            execution_status,
            rpc_execution_status,
            execution_error_message = execution_info.revert_error,
            "execution finished successfully"
        );
    }

    let execution_gas = execution_info.transaction_receipt.fee;
    let rpc_gas = rpc_receipt.actual_fee;
    debug!(?execution_gas, ?rpc_gas, "execution actual fee");
}

fn get_transaction_hashes(network: &str, block_number: u64) -> Result<Vec<String>, RpcStateError> {
    let network = parse_network(network);
    let block_value = BlockValue::Number(BlockNumber(block_number));
    let rpc_state = RpcState::new_rpc(network, block_value)?;
    rpc_state.get_transaction_hashes()
}

fn set_global_subscriber() {
    let default_directive = Directive::from_str("replay=info").expect("should be valid");

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter({
            EnvFilter::builder()
                .with_default_directive(default_directive)
                .from_env_lossy()
        })
        .with_file(false)
        .with_line_number(false);

    #[cfg(feature = "benchmark")]
    let subscriber = subscriber.json();

    #[cfg(not(feature = "benchmark"))]
    let subscriber = subscriber.pretty();

    subscriber.finish().init();
}
