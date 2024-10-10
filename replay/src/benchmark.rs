use std::time::Instant;

use blockifier::{
    context::BlockContext,
    state::{cached_state::CachedState, state_api::StateReader},
};
use rpc_state_reader::{
    blockifier_state_reader::{execute_tx_with_blockifier, fetch_block_context, RpcStateReader},
    rpc_state::{RpcChain, RpcState},
};
use starknet_api::{
    block::BlockNumber,
    hash::StarkHash,
    transaction::{Transaction as SNTransaction, TransactionHash},
};
use tracing::{error, info, info_span};

pub type BlockCachedData = (
    CachedState<OptionalStateReader<RpcStateReader>>,
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
    chain: RpcChain,
) -> Vec<BlockCachedData> {
    let mut block_caches = Vec::new();

    for block_number in block_start.0..=block_end.0 {
        // For each block
        let block_number = BlockNumber(block_number);

        let rpc_state = RpcState::new_rpc(chain, block_number.into()).unwrap();

        // Fetch block context
        let block_context = fetch_block_context(&rpc_state, block_number);

        // Fetch transactions for the block
        let transactions = rpc_state
            .get_transaction_hashes()
            .unwrap()
            .into_iter()
            .map(|transaction_hash| {
                let transaction_hash =
                    TransactionHash(StarkHash::from_hex(&transaction_hash).unwrap());

                // Fetch transaction
                let transaction = rpc_state.get_transaction(&transaction_hash).unwrap();

                (transaction_hash, transaction)
            })
            .collect::<Vec<_>>();

        // Create cached state
        let previous_rpc_state =
            RpcState::new_rpc(chain, block_number.prev().unwrap().into()).unwrap();
        let previous_rpc_state_reader = RpcStateReader::new(previous_rpc_state);
        let cached_state = CachedState::new(OptionalStateReader::new(previous_rpc_state_reader));

        block_caches.push((cached_state, block_context, transactions));
    }

    block_caches
}

/// Executes the given block range, discarding any state changes applied to it
///
/// Can also be used to fill up the cache
pub fn execute_block_range(block_range_data: &mut Vec<BlockCachedData>) {
    for (state, block_context, transactions) in block_range_data {
        // For each block
        let _block_span = info_span!(
            "block execution",
            block_number = block_context.block_info().block_number.0,
        )
        .entered();
        info!("starting block execution");

        // The transactional state is used to execute a transaction while discarding state changes applied to it.
        let mut transactional_state = CachedState::create_transactional(state);

        for (transaction_hash, transaction) in transactions {
            // Execute each transaction
            let _tx_span = info_span!(
                "tx execution",
                transaction_hash = transaction_hash.to_string(),
            )
            .entered();

            info!("tx execution started");

            let pre_execution_instant = Instant::now();
            let result = execute_tx_with_blockifier(
                &mut transactional_state,
                block_context.clone(),
                transaction.to_owned(),
                transaction_hash.to_owned(),
            );
            let execution_time = pre_execution_instant.elapsed();

            match result {
                Ok(info) => {
                    info!(
                        time = ?execution_time,
                        succeeded = info.revert_error.is_none(),
                        "tx execution finished"
                    )
                }
                Err(_) => error!(
                    time = ?execution_time,
                    "tx execution failed unexpectedly"
                ),
            }
        }
    }
}

pub fn fetch_transaction_data(tx: &str, block: BlockNumber, chain: RpcChain) -> BlockCachedData {
    let rpc_state = RpcState::new_rpc(chain, block.into()).unwrap();

    // Fetch block context
    let block_context = fetch_block_context(&rpc_state, block);

    // Fetch transactions for the block
    let transaction_hash = TransactionHash(StarkHash::from_hex(tx).unwrap());
    let transaction = rpc_state.get_transaction(&transaction_hash).unwrap();
    let transactions = vec![(transaction_hash, transaction)];

    // Create cached state
    let previous_rpc_state = RpcState::new_rpc(chain, block.prev().unwrap().into()).unwrap();
    let previous_rpc_state_reader = RpcStateReader::new(previous_rpc_state);
    let cached_state = CachedState::new(OptionalStateReader::new(previous_rpc_state_reader));

    (cached_state, block_context, transactions)
}

/// An implementation of StateReader that can be disabled, panicking if atempted to be read from
///
/// Used to ensure that no requests are made after disabling it.
pub struct OptionalStateReader<S: StateReader>(pub Option<S>);

impl<S: StateReader> OptionalStateReader<S> {
    pub fn new(state_reader: S) -> Self {
        Self(Some(state_reader))
    }

    pub fn get_inner(&self) -> &S {
        self.0
            .as_ref()
            .expect("atempted to read from a disabled state reader")
    }

    pub fn disable(&mut self) {
        self.0 = None;
    }
}

impl<S: StateReader> StateReader for OptionalStateReader<S> {
    fn get_storage_at(
        &self,
        contract_address: starknet_api::core::ContractAddress,
        key: starknet_api::state::StorageKey,
    ) -> blockifier::state::state_api::StateResult<StarkHash> {
        self.get_inner().get_storage_at(contract_address, key)
    }

    fn get_nonce_at(
        &self,
        contract_address: starknet_api::core::ContractAddress,
    ) -> blockifier::state::state_api::StateResult<starknet_api::core::Nonce> {
        self.get_inner().get_nonce_at(contract_address)
    }

    fn get_class_hash_at(
        &self,
        contract_address: starknet_api::core::ContractAddress,
    ) -> blockifier::state::state_api::StateResult<starknet_api::core::ClassHash> {
        self.get_inner().get_class_hash_at(contract_address)
    }

    fn get_compiled_contract_class(
        &self,
        class_hash: starknet_api::core::ClassHash,
    ) -> blockifier::state::state_api::StateResult<
        blockifier::execution::contract_class::ContractClass,
    > {
        self.get_inner().get_compiled_contract_class(class_hash)
    }

    fn get_compiled_class_hash(
        &self,
        class_hash: starknet_api::core::ClassHash,
    ) -> blockifier::state::state_api::StateResult<starknet_api::core::CompiledClassHash> {
        self.get_inner().get_compiled_class_hash(class_hash)
    }
}
