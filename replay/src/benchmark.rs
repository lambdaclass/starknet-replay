use crate::{build_cached_state, get_transaction_hashes};
use blockifier::{
    blockifier::block::BlockInfo,
    context::{BlockContext, ChainInfo},
    state::cached_state::CachedState,
    versioned_constants::VersionedConstants,
};
use rpc_state_reader::blockifier_state_reader::{execute_tx_with_blockifier, RpcStateReader};
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
/// It does not actually execute them, so not all required data will be cached.
/// To ensure that all rpc data is cached, the block range must be execute
/// at least once.
///
/// See `execute_block_range`
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
        let state = build_cached_state(&chain, block_number.0 - 1);

        // Fetch block context
        let block_context = fetch_block_context(&state, block_number);

        // Fetch transactions for the block
        let transactions = get_transaction_hashes(&chain, block_number.0)
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

fn fetch_block_context(
    state: &CachedState<RpcStateReader>,
    block_number: BlockNumber,
) -> BlockContext {
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
            chain_id: state.state.0.get_chain_name().into(),
            fee_token_addresses: Default::default(),
        },
        &VersionedConstants::latest_constants_with_overrides(u32::MAX, usize::MAX),
    )
}

/// Executes the given block range and discards any state changes
///
/// Can be used to fill up the cache
pub fn execute_block_range(block_range_data: &mut Vec<BlockCachedData>) {
    for (state, block_context, transactions) in block_range_data {
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
