use blockifier::{
    state::cached_state::CachedState, transaction::objects::TransactionExecutionInfo,
};
use clap::{Parser, Subcommand};
use rpc_state_reader::{
    blockifier_state_reader::RpcStateReader,
    rpc_state::{BlockValue, RpcChain, RpcState, RpcTransactionReceipt},
    rpc_state_errors::RpcStateError,
};

use rpc_state_reader::blockifier_state_reader::execute_tx_configurable;
#[cfg(feature = "benchmark")]
use rpc_state_reader::{
    execute_tx_configurable_with_state,
    rpc_state::{RpcBlockInfo, RpcState},
    RpcStateReader,
};
use starknet_api::block::BlockNumber;
#[cfg(feature = "benchmark")]
use starknet_api::{
    hash::StarkFelt,
    stark_felt,
    transaction::{Transaction, TransactionHash},
};
#[cfg(feature = "benchmark")]
use starknet_in_rust::{
    definitions::block_context::GasPrices,
    state::{
        cached_state::CachedState, contract_class_cache::PermanentContractClassCache, BlockInfo,
    },
    transaction::Address,
    Felt252,
};
#[cfg(feature = "benchmark")]
use std::ops::Div;
#[cfg(feature = "benchmark")]
use std::{collections::HashMap, sync::Arc, time::Instant};

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
        silent: Option<bool>,
    },
    #[clap(about = "Execute all the transactions in a given block.")]
    Block {
        chain: String,
        block_number: u64,
        silent: Option<bool>,
    },
    #[clap(about = "Execute all the transactions in a given range of blocks.")]
    BlockRange {
        block_start: u64,
        block_end: u64,
        chain: String,
        silent: Option<bool>,
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
        n_runs: usize,
    },
}

fn main() {
    let cli = ReplayCLI::parse();

    match cli.subcommand {
        ReplayExecute::Tx {
            tx_hash,
            chain,
            block_number,
            silent,
        } => {
            let mut state = build_cached_state(&chain, block_number);
            show_execution_data(&mut state, tx_hash, &chain, block_number, silent);
        }
        ReplayExecute::Block {
            block_number,
            chain,
            silent,
        } => {
            println!("Executing block number: {}", block_number);

            let mut state = build_cached_state(&chain, block_number);

            let transaction_hashes = state
                .state
                .0
                .get_transaction_hashes()
                .expect("Unable to fetch the transaction hashes.");

            for tx_hash in transaction_hashes {
                show_execution_data(&mut state, tx_hash, &chain, block_number, silent);
            }
        }
        ReplayExecute::BlockRange {
            block_start,
            block_end,
            chain,
            silent,
        } => {
            println!("Executing block range: {} - {}", block_start, block_end);

            for block_number in block_start..=block_end {
                let mut state = build_cached_state(&chain, block_number);

                let transaction_hashes = state
                    .state
                    .0
                    .get_transaction_hashes()
                    .expect("Unable to fetch the transaction hashes.");

                for tx_hash in transaction_hashes {
                    show_execution_data(&mut state, tx_hash, &chain, block_number, silent);
                }
            }
        }
        #[cfg(feature = "benchmark")]
        ReplayExecute::BenchBlockRange {
            block_start,
            block_end,
            chain,
            n_runs,
        } => {
            println!("Filling up Cache");
            let network = parse_network(&chain);
            // Create a single class_cache for all states
            let class_cache = Arc::new(PermanentContractClassCache::default());
            // HashMaps to cache tx data & states
            let mut transactions =
                HashMap::<BlockNumber, Vec<(TransactionHash, Transaction)>>::new();
            let mut cached_states = HashMap::<
                BlockNumber,
                CachedState<RpcStateReader, PermanentContractClassCache>,
            >::new();
            let mut block_timestamps = HashMap::<BlockNumber, u64>::new();
            let mut sequencer_addresses = HashMap::<BlockNumber, Address>::new();
            let mut gas_prices = HashMap::<BlockNumber, GasPrices>::new();
            for block_number in block_start..=block_end {
                // For each block:
                let block_number = BlockNumber(block_number);
                // Create a cached state
                let rpc_reader =
                    RpcStateReader::new(RpcState::new_rpc(network, block_number.into()).unwrap());
                let mut state = CachedState::new(Arc::new(rpc_reader), class_cache.clone());
                // Fetch block timestamps & sequencer address
                let RpcBlockInfo {
                    block_timestamp,
                    sequencer_address,
                    ..
                } = state.state_reader.0.get_block_info().unwrap();
                block_timestamps.insert(block_number, block_timestamp.0);
                let sequencer_address = Address(Felt252::from_bytes_be_slice(
                    sequencer_address.0.key().bytes(),
                ));
                sequencer_addresses.insert(block_number, sequencer_address.clone());
                // Fetch gas price
                let gas_price = state.state_reader.0.get_gas_price(block_number.0).unwrap();
                gas_prices.insert(block_number, gas_price.clone());

                // Fetch txs for the block
                let transaction_hashes = get_transaction_hashes(block_number, network)
                    .expect("Unable to fetch the transaction hashes.");
                let mut txs_in_block = Vec::<(TransactionHash, Transaction)>::new();
                for tx_hash in transaction_hashes {
                    // Fetch tx and add it to txs_in_block cache
                    let tx_hash = TransactionHash(stark_felt!(tx_hash.strip_prefix("0x").unwrap()));
                    let tx = state.state_reader.0.get_transaction(&tx_hash).unwrap();
                    txs_in_block.push((tx_hash, tx.clone()));
                    // First execution to fill up cache values
                    let _ = execute_tx_configurable_with_state(
                        &tx_hash,
                        tx.clone(),
                        network,
                        BlockInfo {
                            block_number: block_number.0,
                            block_timestamp: block_timestamp.0,
                            gas_price: gas_price.clone(),
                            sequencer_address: sequencer_address.clone(),
                        },
                        false,
                        true,
                        &mut state,
                    );
                }
                // Add the txs from the current block to the transactions cache
                transactions.insert(block_number, txs_in_block);
                // Clean writes from cached_state
                state.cache_mut().storage_writes_mut().clear();
                state.cache_mut().class_hash_writes_mut().clear();
                state.cache_mut().nonce_writes_mut().clear();
                // Add the cached state for the current block to the cached_states cache
                cached_states.insert(block_number, state);
            }
            // Benchmark run should make no api requests as all data is cached

            println!(
                "Executing block range: {} - {} {} times",
                block_start, block_end, n_runs
            );
            let now = Instant::now();
            for _ in 0..n_runs {
                for block_number in block_start..=block_end {
                    let block_number = BlockNumber(block_number);
                    // Fetch state
                    let state = cached_states.get_mut(&block_number).unwrap();
                    // Fetch txs
                    let block_txs = transactions.get(&block_number).unwrap();
                    // Fetch timestamp
                    let block_timestamp = *block_timestamps.get(&block_number).unwrap();
                    // Fetch sequencer address
                    let sequencer_address = sequencer_addresses.get(&block_number).unwrap();
                    // Fetch gas price
                    let gas_price = gas_prices.get(&block_number).unwrap();
                    // Run txs
                    for (tx_hash, tx) in block_txs {
                        let _ = execute_tx_configurable_with_state(
                            tx_hash,
                            tx.clone(),
                            network,
                            BlockInfo {
                                block_number: block_number.0,
                                block_timestamp,
                                gas_price: gas_price.clone(),
                                sequencer_address: sequencer_address.clone(),
                            },
                            false,
                            true,
                            state,
                        );
                    }
                }
            }
            let elapsed_time = now.elapsed();
            println!(
                "Ran blocks {} - {} {} times in {} seconds. Approximately {} second(s) per run",
                block_start,
                block_end,
                n_runs,
                elapsed_time.as_secs_f64(),
                elapsed_time.as_secs_f64().div(n_runs as f64)
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

fn build_cached_state(network: &str, current_block_number: u64) -> CachedState<RpcStateReader> {
    let previous_block_number = BlockNumber(current_block_number - 1);
    let rpc_chain = parse_network(&network);
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
    silent: Option<bool>,
) {
    if silent.is_none() || !silent.unwrap() {
        println!("Executing transaction with hash: {}", tx_hash);
        println!("Block number: {}", block_number);
        println!("Chain: {}", chain);
    }
    let previous_block_number = BlockNumber(block_number - 1);

    let (tx_info, _trace, receipt) =
        match execute_tx_configurable(state, &tx_hash, previous_block_number, false, true) {
            Ok(x) => x,
            Err(error_reason) => {
                println!("Error: {}", error_reason);
                return;
            }
        };
    let TransactionExecutionInfo {
        revert_error,
        actual_fee,
        ..
    } = tx_info;

    let sir_actual_fee = actual_fee;

    let RpcTransactionReceipt {
        actual_fee,
        execution_status,
        ..
    } = receipt;

    if silent.is_none() || !silent.unwrap() {
        println!("[RPC] Execution status: {:?}", execution_status);
        if let Some(revert_error) = revert_error {
            println!("[SIR] Revert error: {}", revert_error);
        }
        println!(
            "[RPC] Actual fee: {} {}",
            actual_fee.amount, actual_fee.unit
        );
        println!("[SIR] Actual fee: {:?} wei", sir_actual_fee);
    }
}

fn get_transaction_hashes(
    block_number: BlockNumber,
    network: RpcChain,
) -> Result<Vec<String>, RpcStateError> {
    let rpc_state = RpcState::new_rpc(network, BlockValue::Number(block_number))?;
    rpc_state.get_transaction_hashes()
}
