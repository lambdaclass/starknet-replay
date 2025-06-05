use std::time::{Duration, Instant};

use blockifier::{
    context::BlockContext,
    execution::{
        call_info::CallInfo,
        contract_class::{RunnableCompiledClass, TrackedResource},
    },
    state::{
        cached_state::CachedState, errors::StateError,
        state_api::StateReader as BlockifierStateReader,
    },
    transaction::{
        account_transaction::ExecutionFlags, objects::TransactionExecutionInfo,
        transaction_execution::Transaction, transactions::ExecutableTransaction,
    },
};
use rpc_state_reader::{
    cache::RpcCachedStateReader,
    execution::{fetch_block_context, fetch_blockifier_transaction},
    reader::{RpcStateReader, StateReader},
};
use serde::Serialize;
use starknet_api::{
    block::BlockNumber,
    core::{ChainId, ClassHash, EntryPointSelector},
    felt,
    hash::StarkHash,
    transaction::TransactionHash,
};

pub type BlockCachedData = (
    CachedState<OptionalStateReader<RpcCachedStateReader>>,
    BlockContext,
    Vec<Transaction>,
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
    chain: ChainId,
) -> Vec<BlockCachedData> {
    let mut block_caches = Vec::new();

    for block_number in block_start.0..=block_end.0 {
        // For each block
        let block_number = BlockNumber(block_number);
        let reader = RpcCachedStateReader::new(RpcStateReader::new(chain.clone(), block_number));

        // Fetch block context
        let block_context = fetch_block_context(&reader).unwrap();

        let flags = ExecutionFlags {
            only_query: false,
            charge_fee: false,
            validate: true,
        };

        // Fetch transactions for the block
        let transactions = reader
            .get_block_with_tx_hashes()
            .unwrap()
            .transactions
            .into_iter()
            .map(|hash| fetch_blockifier_transaction(&reader, flags.clone(), hash).unwrap())
            .collect::<Vec<_>>();

        // Create cached state
        let previous_block_number = block_number.prev().unwrap();
        let previous_reader =
            RpcCachedStateReader::new(RpcStateReader::new(chain.clone(), previous_block_number));
        let cached_state = CachedState::new(OptionalStateReader::new(previous_reader));

        block_caches.push((cached_state, block_context, transactions));
    }

    block_caches
}

/// Executes the given block range, discarding any state changes applied to it
///
/// Can also be used to fill up the cache
pub fn execute_block_range(
    block_range_data: &mut Vec<BlockCachedData>,
) -> Vec<(TransactionHash, TransactionExecutionInfo, Duration)> {
    let mut executions = Vec::new();

    for (state, block_context, transactions) in block_range_data {
        // For each block

        // The transactional state is used to execute a transaction while discarding state changes applied to it.
        let mut transactional_state = CachedState::create_transactional(state);

        for transaction in transactions {
            // Execute each transaction

            let pre_execution_instant = Instant::now();
            let execution = transaction.execute(&mut transactional_state, block_context);
            let Ok(execution) = execution else { continue };
            let execution_time = pre_execution_instant.elapsed();

            executions.push((Transaction::tx_hash(transaction), execution, execution_time));
        }
    }

    executions
}

#[derive(Serialize)]
pub struct BenchmarkingData {
    pub transaction_executions: Vec<TransactionExecutionBenchmark>,
    pub class_executions: Vec<ClassExecutionInfo>,
}

#[derive(Serialize)]
pub struct ClassExecutionInfo {
    class_hash: ClassHash,
    selector: EntryPointSelector,
    time_ns: u128,
    gas_consumed: u64,
    steps: u64,
    resource: TrackedResource,
}

#[derive(Serialize)]
pub struct TransactionExecutionBenchmark {
    hash: TransactionHash,
    time_ns: u128,
    gas_consumed: u64,
    steps: u64,
    first_class: usize,
}

pub fn aggregate_executions(
    executions: Vec<(TransactionHash, TransactionExecutionInfo, Duration)>,
) -> BenchmarkingData {
    let mut class_executions = vec![];
    let mut transaction_executions = vec![];

    for (hash, execution, time) in executions {
        let first_class_index = class_executions.len();

        let mut gas_consumed = 0;
        let mut steps = 0;

        if let Some(call) = execution.validate_call_info {
            gas_consumed += call.execution.gas_consumed;
            steps += call.resources.n_steps as u64;
            class_executions.append(&mut get_class_executions(call));
        }
        if let Some(call) = execution.execute_call_info {
            gas_consumed += call.execution.gas_consumed;
            steps += call.resources.n_steps as u64;
            class_executions.append(&mut get_class_executions(call));
        }
        if let Some(call) = execution.fee_transfer_call_info {
            gas_consumed += call.execution.gas_consumed;
            steps += call.resources.n_steps as u64;
            class_executions.append(&mut get_class_executions(call));
        }

        transaction_executions.push(TransactionExecutionBenchmark {
            hash,
            time_ns: time.as_nanos(),
            first_class: first_class_index,
            gas_consumed,
            steps,
        });
    }

    BenchmarkingData {
        transaction_executions,
        class_executions,
    }
}

fn get_class_executions(call: CallInfo) -> Vec<ClassExecutionInfo> {
    // class hash can initially be None, but it is always added before execution
    let class_hash = call.call.class_hash.unwrap();

    let mut inner_time = Duration::ZERO;
    let mut inner_steps = 0;
    let mut inner_gas_consumed = 0;

    let mut classes = call
        .inner_calls
        .into_iter()
        .flat_map(|call| {
            inner_time += call.time;
            inner_gas_consumed += call.execution.gas_consumed;
            inner_steps += call.resources.n_steps as u64;
            get_class_executions(call)
        })
        .collect::<Vec<_>>();

    let time = call
        .time
        .checked_sub(inner_time)
        .expect("time cannot be negative");

    let gas_consumed = call
        .execution
        .gas_consumed
        .checked_sub(inner_gas_consumed)
        .expect("gas cannot be negative");

    let steps = (call.resources.n_steps as u64)
        .checked_sub(inner_steps)
        .expect("gas cannot be negative");

    let top_class = ClassExecutionInfo {
        class_hash,
        selector: call.call.entry_point_selector,
        time_ns: time.as_nanos(),
        gas_consumed,
        resource: call.tracked_resource,
        steps,
    };

    classes.push(top_class);

    classes
}

pub fn fetch_transaction_data(tx: &str, block: BlockNumber, chain: ChainId) -> BlockCachedData {
    let reader = RpcCachedStateReader::new(RpcStateReader::new(chain.clone(), block));

    // Fetch block context
    let block_context = fetch_block_context(&reader).unwrap();

    let flags = ExecutionFlags {
        only_query: false,
        charge_fee: false,
        validate: true,
    };

    // Fetch transaction
    let tx_hash = TransactionHash(felt!(tx));
    let transaction = fetch_blockifier_transaction(&reader, flags.clone(), tx_hash).unwrap();
    let transactions = vec![transaction];

    // Create cached state
    let previous_block_number = block.prev().unwrap();
    let previous_reader =
        RpcCachedStateReader::new(RpcStateReader::new(chain, previous_block_number));
    let cached_state = CachedState::new(OptionalStateReader::new(previous_reader));

    (cached_state, block_context, transactions)
}

/// An implementation of StateReader that can be disabled, panicking if atempted to be read from
///
/// Used to ensure that no requests are made after disabling it.
pub struct OptionalStateReader<S: BlockifierStateReader>(pub Option<S>);

impl<S: BlockifierStateReader> OptionalStateReader<S> {
    pub fn new(state_reader: S) -> Self {
        Self(Some(state_reader))
    }

    pub fn get_inner(&self) -> Result<&S, StateError> {
        self.0.as_ref().ok_or(StateError::StateReadError(
            "Atempted to read from a disabled state reader".to_string(),
        ))
    }

    pub fn disable(&mut self) {
        self.0 = None;
    }
}

impl<S: BlockifierStateReader> BlockifierStateReader for OptionalStateReader<S> {
    fn get_storage_at(
        &self,
        contract_address: starknet_api::core::ContractAddress,
        key: starknet_api::state::StorageKey,
    ) -> blockifier::state::state_api::StateResult<StarkHash> {
        self.get_inner()?.get_storage_at(contract_address, key)
    }

    fn get_nonce_at(
        &self,
        contract_address: starknet_api::core::ContractAddress,
    ) -> blockifier::state::state_api::StateResult<starknet_api::core::Nonce> {
        self.get_inner()?.get_nonce_at(contract_address)
    }

    fn get_class_hash_at(
        &self,
        contract_address: starknet_api::core::ContractAddress,
    ) -> blockifier::state::state_api::StateResult<starknet_api::core::ClassHash> {
        self.get_inner()?.get_class_hash_at(contract_address)
    }

    fn get_compiled_class(
        &self,
        class_hash: starknet_api::core::ClassHash,
    ) -> blockifier::state::state_api::StateResult<RunnableCompiledClass> {
        self.get_inner()?.get_compiled_class(class_hash)
    }

    fn get_compiled_class_hash(
        &self,
        class_hash: starknet_api::core::ClassHash,
    ) -> blockifier::state::state_api::StateResult<starknet_api::core::CompiledClassHash> {
        self.get_inner()?.get_compiled_class_hash(class_hash)
    }
}
