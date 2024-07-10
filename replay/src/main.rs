use std::str::FromStr;

use blockifier::state::cached_state::CachedState;
use clap::{Parser, Subcommand};
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
    blockifier::{
        blockifier::block::BlockInfo,
        context::{BlockContext, ChainInfo},
        versioned_constants::VersionedConstants,
    },
    rpc_state_reader::blockifier_state_reader::execute_tx_with_blockifier,
    starknet_api::{hash::StarkFelt, transaction::TransactionHash},
    std::{ops::Div, time::Instant},
};

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
            info!("filling up cache");

            let chain_id = parse_network(&chain);

            let mut block_caches = Vec::new();

            for block_number in block_start..=block_end {
                // For each block
                let block_number = BlockNumber(block_number);

                // Create a cached state
                let mut state = build_cached_state(&chain, block_number.0 - 1);

                // Fetch block context
                let block_context = {
                    let rpc_block_info = state.state.0.get_block_info().unwrap();

                    let gas_price = state.state.0.get_gas_price(block_number.0).unwrap();

                    BlockContext::new_unchecked(
                        &BlockInfo {
                            block_number,
                            block_timestamp: rpc_block_info.block_timestamp,
                            sequencer_address: rpc_block_info.sequencer_address,
                            gas_prices: gas_price,
                            use_kzg_da: false,
                        },
                        &ChainInfo {
                            chain_id: chain_id.into(),
                            fee_token_addresses: Default::default(),
                        },
                        &VersionedConstants::latest_constants_with_overrides(u32::MAX, usize::MAX),
                    )
                };

                // Fetch transactions for the block
                let transactions = get_transaction_hashes(&chain, block_number.0)
                    .unwrap()
                    .into_iter()
                    .map(|transaction_hash| {
                        let transaction_hash = TransactionHash(
                            StarkFelt::try_from(transaction_hash.strip_prefix("0x").unwrap())
                                .unwrap(),
                        );

                        // Fetch transaction
                        let transaction = state.state.0.get_transaction(&transaction_hash).unwrap();

                        (transaction_hash, transaction)
                    })
                    .collect::<Vec<_>>();

                // The transactional state is used to execute a transaction while discarding all writes to it.
                let mut transactional_state = CachedState::create_transactional(&mut state);

                for (transaction_hash, transaction) in &transactions {
                    // Execute each transaction
                    // Internally, this fetches all the needed information and caches it
                    let result = execute_tx_with_blockifier(
                        &mut transactional_state,
                        block_context.clone(),
                        transaction.to_owned(),
                        transaction_hash.to_owned(),
                    );

                    match result {
                        Ok(info) => {
                            info!(
                                transaction_hash = transaction_hash.to_string(),
                                succeeded = info.revert_error.is_none(),
                                "tx execution status"
                            )
                        }
                        Err(_) => error!(
                            transaction_hash = transaction_hash.to_string(),
                            "tx execution failed"
                        ),
                    }
                }

                block_caches.push((state, block_context, transactions));
            }

            let before_execution = Instant::now();

            info!("replaying with cached state");

            // Benchmark run should make no api requests as all data is cached

            for _ in 0..number_of_runs {
                for (state, block_context, transactions) in &mut block_caches {
                    // The transactional state is used to execute a transaction while discarding all writes to it.
                    let mut transactional_state = CachedState::create_transactional(state);

                    for (transaction_hash, transaction) in transactions {
                        let result = execute_tx_with_blockifier(
                            &mut transactional_state,
                            block_context.clone(),
                            transaction.to_owned(),
                            transaction_hash.to_owned(),
                        );

                        match result {
                            Ok(info) => {
                                info!(
                                    transaction_hash = transaction_hash.to_string(),
                                    succeeded = info.revert_error.is_none(),
                                    "tx execution status"
                                )
                            }
                            Err(_) => error!(
                                transaction_hash = transaction_hash.to_string(),
                                "tx execution failed"
                            ),
                        }
                    }
                }
            }

            let execution_time = before_execution.elapsed();
            let total_run_time = execution_time.as_secs_f64();
            let average_run_time = total_run_time.div(number_of_runs as f64);
            info!(
                block_start,
                block_end, number_of_runs, total_run_time, average_run_time, "benchmark finished",
            );
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

    let execution_gas = execution_info.actual_fee;
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

    tracing_subscriber::fmt()
        .with_env_filter({
            EnvFilter::builder()
                .with_default_directive(default_directive)
                .from_env_lossy()
        })
        .pretty()
        .with_file(false)
        .with_line_number(false)
        .finish()
        .init();
}
