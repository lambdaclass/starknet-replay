use blockifier::transaction::objects::TransactionExecutionInfo;
use clap::{Parser, Subcommand};
use rpc_state_reader::{
    rpc_state::{BlockValue, RpcChain, RpcState, RpcTransactionReceipt,RpcBlockInfo},
    rpc_state_errors::RpcStateError,
};

use rpc_state_reader::blockifier_state_reader::{
    RpcStateReader,
    execute_tx_configurable,
    execute_tx_configurable_with_state,
};
use starknet_api::{block::{BlockNumber, BlockTimestamp}, core::ContractAddress};

use starknet_api::{
    hash::StarkFelt,
    stark_felt,
    transaction::{Transaction, TransactionHash},
};

use blockifier::blockifier::{block::GasPrices,block::BlockInfo};
use blockifier::state::cached_state::CachedState;

use std::ops::Div;
use std::{collections::HashMap,time::Instant};

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
    //#[cfg(feature = "benchmark")]
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
            show_execution_data(tx_hash, &chain, block_number, silent);
        }
        ReplayExecute::Block {
            block_number,
            chain,
            silent,
        } => {
            println!("Executing block number: {}", block_number);
            let rpc_chain = parse_network(&chain);
            let block_number = BlockNumber(block_number);
            let transaction_hashes = get_transaction_hashes(block_number, rpc_chain)
                .expect("Unable to fetch the transaction hashes.");

            for tx_hash in transaction_hashes {
                show_execution_data(tx_hash, &chain, block_number.0, silent);
            }
        }
        ReplayExecute::BlockRange {
            block_start,
            block_end,
            chain,
            silent,
        } => {
            println!("Executing block range: {} - {}", block_start, block_end);
            let rpc_chain = parse_network(&chain);
            for block_number in block_start..=block_end {
                let block_number = BlockNumber(block_number);
                let transaction_hashes = get_transaction_hashes(block_number, rpc_chain)
                    .expect("Unable to fetch the transaction hashes.");

                for tx_hash in transaction_hashes {
                    show_execution_data(tx_hash, &chain, block_number.0, silent);
                }
            }
        }
        //#[cfg(feature = "benchmark")]
        ReplayExecute::BenchBlockRange {
            block_start,
            block_end,
            chain,
            n_runs,
        } => {
            println!("Filling up Cache");
            let network = parse_network(&chain);
            let mut transactions =
                HashMap::<BlockNumber, Vec<(TransactionHash, Transaction)>>::new();
            let mut cached_states = HashMap::<
                BlockNumber,
                CachedState<RpcStateReader>,
            >::new();
            let mut block_timestamps = HashMap::<BlockNumber, u64>::new();
            let mut sequencer_addresses = HashMap::<BlockNumber, ContractAddress>::new();
            let mut gas_prices = HashMap::<BlockNumber, GasPrices>::new();
            for block_number in block_start..=block_end {
                println!("Executing block: {}", block_number);
                // For each block:
                let block_number = BlockNumber(block_number);
                // Create a cached state
                let rpc_reader =
                    RpcStateReader::new(RpcState::new_rpc(network, block_number.into()).unwrap());
                let mut state = CachedState::new(rpc_reader.clone());
                // Fetch block timestamps & sequencer address
                let RpcBlockInfo {
                    block_timestamp,
                    sequencer_address,
                    ..
                } = rpc_reader.0.get_block_info().unwrap();
                block_timestamps.insert(block_number, block_timestamp.0);
                
                let sequencer_address = ContractAddress(
                    sequencer_address.0
                );

                sequencer_addresses.insert(block_number, sequencer_address.clone());
                // Fetch gas price
                let gas_price = rpc_reader.0.get_gas_price(block_number.0).unwrap();
                gas_prices.insert(block_number, gas_price.clone());

                // Fetch txs for the block
                let transaction_hashes = get_transaction_hashes(block_number, network)
                    .expect("Unable to fetch the transaction hashes.");
                let mut txs_in_block = Vec::<(TransactionHash, Transaction)>::new();
                
                for tx_hash in transaction_hashes {
                    // Fetch tx and add it to txs_in_block cache
                    let tx_hash = TransactionHash(stark_felt!(tx_hash.strip_prefix("0x").unwrap()));
                    let tx = rpc_reader.0.get_transaction(&tx_hash).unwrap();
                    txs_in_block.push((tx_hash, tx.clone()));
                    // First execution to fill up cache values
                    let _ = execute_tx_configurable_with_state(
                        &tx_hash,
                        tx.clone(),
                        network,
                        BlockInfo {
                            block_number,
                            block_timestamp,
                            sequencer_address: sequencer_address.clone(),
                            gas_prices: gas_price.clone(),
                            use_kzg_da: false,
                        },
                        false,
                        true,
                        &mut state,
                    );
                }
                // Add the txs from the current block to the transactions cache
                transactions.insert(block_number, txs_in_block);
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
                                block_number,
                                block_timestamp: BlockTimestamp(block_timestamp),
                                sequencer_address: sequencer_address.clone(),
                                gas_prices: gas_price.clone(),
                                use_kzg_da: false,
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

fn show_execution_data(tx_hash: String, chain: &str, block_number: u64, silent: Option<bool>) {
    let rpc_chain = parse_network(chain);
    if silent.is_none() || !silent.unwrap() {
        println!("Executing transaction with hash: {}", tx_hash);
        println!("Block number: {}", block_number);
        println!("Chain: {}", chain);
    }
    let previous_block_number = BlockNumber(block_number - 1);

    let (tx_info, _trace, receipt) =
        match execute_tx_configurable(&tx_hash, rpc_chain, previous_block_number, false, true) {
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
