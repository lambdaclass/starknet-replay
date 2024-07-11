use crate::{build_cached_state, get_transaction_hashes};
use blockifier::{context::BlockContext, state::cached_state::CachedState};
use rpc_state_reader::blockifier_state_reader::{
    execute_tx_with_blockifier, fetch_block_context, RpcStateReader,
};
use starknet_api::{
    block::BlockNumber,
    hash::StarkFelt,
    transaction::{Transaction as SNTransaction, TransactionHash},
};
use tracing::{error, info};

pub type BlockCachedData = (
    CachedState<RpcStateReader>,
    BlockContext,
    Vec<(TransactionHash, SNTransaction)>,
);

/// Fetches context data to execute the given block range
///
/// It does not actually execute the block range, so not all data required
/// by blockifier will be cached. To ensure that all rpc data is cached,
/// the block range must be executed once.
///
/// See `execute_block_range` to execute the block range
pub fn fetch_block_range_data(
    block_start: BlockNumber,
    block_end: BlockNumber,
    chain: &str,
) -> Vec<BlockCachedData> {
    let mut block_caches = Vec::new();

    for block_number in block_start.0..=block_end.0 {
        // For each block
        let block_number = BlockNumber(block_number);

        // Create a cached state
        let state = build_cached_state(chain, block_number.0 - 1);

        // Fetch block context
        let block_context = fetch_block_context(&state, block_number);

        // Fetch transactions for the block
        let transactions = get_transaction_hashes(chain, block_number.0)
            .unwrap()
            .into_iter()
            .map(|transaction_hash| {
                let transaction_hash = TransactionHash(
                    StarkFelt::try_from(transaction_hash.strip_prefix("0x").unwrap()).unwrap(),
                );

                // Fetch transaction
                let transaction = state.state.0.get_transaction(&transaction_hash).unwrap();

                (transaction_hash, transaction)
            })
            .collect::<Vec<_>>();

        block_caches.push((state, block_context, transactions));
    }

    block_caches
}

/// Executes the given block range, discarding any state changes applied to it
///
/// Can also be used to fill up the cache
pub fn execute_block_range(block_range_data: &mut Vec<BlockCachedData>) {
    for (state, block_context, transactions) in block_range_data {
        // For each block

        // The transactional state is used to execute a transaction while discarding state changes applied to it.
        let mut transactional_state = CachedState::create_transactional(state);

        for (transaction_hash, transaction) in transactions {
            // Execute each transaction
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
