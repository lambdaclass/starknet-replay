use std::time::{Duration, Instant};

use blockifier::{
    blockifier::block::validated_gas_prices,
    blockifier_versioned_constants::VersionedConstants,
    bouncer::BouncerConfig,
    context::BlockContext,
    state::{cached_state::CachedState, state_api::StateReader},
    transaction::{
        account_transaction::ExecutionFlags,
        objects::{TransactionExecutionInfo, TransactionExecutionResult},
        transaction_execution::Transaction as BlockifierTransaction,
        transactions::ExecutableTransaction,
    },
};
use blockifier_reexecution::state_reader::utils::get_chain_info;
use serde::Serialize;
use starknet_api::{
    block::{BlockInfo, BlockNumber, BlockTimestamp, GasPrice, NonzeroGasPrice, StarknetVersion},
    core::{ContractAddress, PatriciaKey},
    test_utils::MAX_FEE,
    transaction::{Transaction, TransactionHash},
};
use starknet_core::types::{BlockWithTxHashes, Felt};
use state_reader::{
    block_state_reader::BlockStateReader, full_state_reader::FullStateReader,
    objects::RpcTransactionReceipt,
};
use tracing::{error, info, info_span};

// the fields are used by the benchmark feature
// so we allow dead code to silence warnings.
#[allow(dead_code)]
pub struct TransactionExecution {
    pub result: TransactionExecutionResult<TransactionExecutionInfo>,
    pub time: Duration,
    pub hash: TransactionHash,
    pub block_number: BlockNumber,
}

pub fn execute_block(
    reader: &FullStateReader,
    block_number: BlockNumber,
    execution_flags: ExecutionFlags,
) -> anyhow::Result<Vec<TransactionExecution>> {
    let _block_execution_span = info_span!("block execution", block = block_number.0).entered();

    let block = reader.get_block(block_number)?;
    let tx_hashes: Vec<TransactionHash> = block
        .transactions
        .into_iter()
        .map(TransactionHash)
        .collect();

    execute_txs(reader, block_number, tx_hashes, execution_flags)
}

pub fn execute_txs(
    reader: &FullStateReader,
    block_number: BlockNumber,
    tx_hashes: Vec<TransactionHash>,
    execution_flags: ExecutionFlags,
) -> anyhow::Result<Vec<TransactionExecution>> {
    let block_reader = BlockStateReader::new(
        block_number
            .prev()
            .expect("block number should not be zero"),
        reader,
    );
    let mut state = CachedState::new(block_reader);
    let block_context = get_block_context(reader, block_number)?;

    let mut executions = vec![];

    for tx_hash in tx_hashes {
        executions.push(execute_tx(
            &mut state,
            reader,
            &block_context,
            tx_hash,
            execution_flags.clone(),
        )?);
    }

    Ok(executions)
}

pub fn execute_tx(
    state: &mut CachedState<impl StateReader>,
    reader: &FullStateReader,
    block_context: &BlockContext,
    tx_hash: TransactionHash,
    execution_flags: ExecutionFlags,
) -> anyhow::Result<TransactionExecution> {
    let _transaction_execution_span =
        info_span!("transaction execution", hash = tx_hash.to_hex_string(),).entered();

    let tx = get_blockifier_transaction(
        reader,
        block_context.block_info().block_number,
        tx_hash,
        execution_flags,
    )
    .inspect_err(|err| error!("failed to fetch transaction: {}", err))?;

    info!("starting transaction execution");

    let pre_execute_instant = Instant::now();
    let execution_result = tx.execute(state, block_context);
    let execution_time = pre_execute_instant.elapsed();

    // TODO: Move this to the caller.
    // This function should only execute the transaction and return relevant information.
    #[cfg(feature = "state_dump")]
    crate::state_dump::create_state_dump(
        state,
        block_context.block_info().block_number.0,
        &tx_hash.to_hex_string(),
        &execution_result,
    );

    // TODO: Move this to the caller.
    // This function should only execute the transaction and return relevant information.
    #[cfg(feature = "with-libfunc-profiling")]
    crate::libfunc_profile::create_libfunc_profile(tx_hash.to_hex_string());

    match &execution_result {
        Ok(execution_info) => {
            let result_string = execution_info
                .revert_error
                .as_ref()
                .map(|err| err.to_string())
                .unwrap_or("ok".to_string());

            info!("transaction execution finished: {}", result_string);

            let receipt = reader.get_tx_receipt(tx_hash)?;
            validate_tx_with_receipt(execution_info, receipt);
        }
        Err(err) => error!("transaction execution failed: {err}"),
    }

    Ok(TransactionExecution {
        result: execution_result,
        time: execution_time,
        hash: tx_hash,
        block_number: block_context.block_info().block_number,
    })
}

/// Validates the transaction execution with the network receipt,
/// and logs the result
fn validate_tx_with_receipt(
    execution_info: &TransactionExecutionInfo,
    receipt: RpcTransactionReceipt,
) {
    #[derive(PartialEq, Debug, Serialize)]
    struct TransactionSummary {
        reverted: bool,
        events: usize,
        messages: usize,
    }

    let actual_execution_summary = TransactionSummary {
        reverted: execution_info.is_reverted(),
        events: execution_info
            .receipt
            .resources
            .starknet_resources
            .archival_data
            .event_summary
            .n_events
            + 1,
        messages: execution_info
            .receipt
            .resources
            .starknet_resources
            .messages
            .l2_to_l1_payload_lengths
            .len(),
    };
    let expected_execution_summary = TransactionSummary {
        reverted: receipt.execution_result.revert_reason().is_some(),
        events: receipt.events.len(),
        messages: receipt.messages_sent.len(),
    };

    if actual_execution_summary == expected_execution_summary {
        info!(
            "execution summary coincides with network: {}",
            serde_json::to_string(&actual_execution_summary)
                .expect("serializing simple struct cannot fail"),
        )
    } else {
        error!(
            "execution summary differs with network: {} vs. {}",
            serde_json::to_string(&actual_execution_summary)
                .expect("serializing simple struct cannot fail"),
            serde_json::to_string(&expected_execution_summary)
                .expect("serializing simple struct cannot fail"),
        )
    }
}

pub fn get_block_context(
    reader: &FullStateReader,
    block_number: BlockNumber,
) -> anyhow::Result<BlockContext> {
    let block = reader.get_block(block_number)?;
    let chain_id = reader.get_chain_id()?;

    let block_info = get_block_info(&block)?;
    let chain_info = get_chain_info(&chain_id);
    let versioned_constants = get_versioned_constants(&block)?;

    Ok(BlockContext::new(
        block_info,
        chain_info,
        versioned_constants,
        BouncerConfig::max(),
    ))
}

pub fn get_blockifier_transaction(
    reader: &FullStateReader,
    block_number: BlockNumber,
    tx_hash: TransactionHash,
    execution_flags: ExecutionFlags,
) -> anyhow::Result<BlockifierTransaction> {
    let tx = reader.get_tx(tx_hash)?;

    let class_info = if let Transaction::Declare(declare_tx) = &tx {
        Some(reader.get_class_info(block_number, declare_tx.class_hash())?)
    } else {
        None
    };

    let fee = if let Transaction::L1Handler(_) = tx {
        Some(MAX_FEE)
    } else {
        None
    };

    Ok(BlockifierTransaction::from_api(
        tx,
        tx_hash,
        class_info,
        fee,
        None,
        execution_flags,
    )?)
}

pub fn get_versioned_constants(block: &BlockWithTxHashes) -> anyhow::Result<VersionedConstants> {
    let version = StarknetVersion::try_from(block.starknet_version.as_str())?;

    Ok(VersionedConstants::get(&version)
        .unwrap_or_else(|_| VersionedConstants::latest_constants())
        .clone())
}

pub fn get_block_info(block: &BlockWithTxHashes) -> anyhow::Result<BlockInfo> {
    fn parse_gas_price(price: Felt) -> anyhow::Result<NonzeroGasPrice> {
        let price = GasPrice(u128::try_from(price)?);
        Ok(NonzeroGasPrice::new(price).unwrap_or(NonzeroGasPrice::MIN))
    }

    Ok(BlockInfo {
        block_number: BlockNumber(block.block_number),
        sequencer_address: ContractAddress(PatriciaKey::try_from(block.sequencer_address)?),
        block_timestamp: BlockTimestamp(block.timestamp),
        gas_prices: validated_gas_prices(
            parse_gas_price(block.l1_gas_price.price_in_wei)?,
            parse_gas_price(block.l1_gas_price.price_in_fri)?,
            parse_gas_price(block.l1_data_gas_price.price_in_wei)?,
            parse_gas_price(block.l1_data_gas_price.price_in_fri)?,
            parse_gas_price(block.l2_gas_price.price_in_wei)?,
            parse_gas_price(block.l2_gas_price.price_in_fri)?,
        ),
        use_kzg_da: true,
    })
}

#[cfg(test)]
mod tests {
    use blockifier::{
        state::cached_state::CachedState, transaction::account_transaction::ExecutionFlags,
    };
    use starknet_api::{block::BlockNumber, core::ChainId, felt, transaction::TransactionHash};
    use state_reader::{block_state_reader::BlockStateReader, full_state_reader::FullStateReader};
    use test_case::test_case;

    use crate::execution::{execute_tx, get_block_context};

    #[test_case(
        "0x04ba569a40a866fd1cbb2f3d3ba37ef68fb91267a4931a377d6acc6e5a854f9a",
        648462,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x0780e3a498b4fd91ab458673891d3e8ee1453f9161f4bfcb93dd1e2c91c52e10",
        650558,
        ChainId::Mainnet
    )]
    fn execute_transaction(hash: &str, block_number: u64, chain: ChainId) {
        let tx_hash = TransactionHash(felt!(hash));
        let block_number = BlockNumber(block_number);

        let full_reader = FullStateReader::new(chain);
        let block_reader = BlockStateReader::new(
            block_number
                .prev()
                .expect("block number should not be zero"),
            &full_reader,
        );
        let mut state = CachedState::new(block_reader);
        let block_context =
            get_block_context(&full_reader, block_number).expect("failed to get block context");

        let flags = ExecutionFlags {
            only_query: false,
            charge_fee: false,
            validate: false,
            strict_nonce_check: true,
        };
        let execution = execute_tx(&mut state, &full_reader, &block_context, tx_hash, flags)
            .expect("failed to execute transaction")
            .result
            .expect("transaction execution failed");

        // TODO: We should execute with both Cairo Native and Cairo VM, and
        // compare the execution result and state diffs.
        assert!(!execution.is_reverted())
    }

    #[test_case(
        "0x01fa98bed3aff8d9bb109fff1215b15b60dd2ca75045a5c5362655a0c380ef98",
        500000,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x02b28b4846a756e0cec6385d6d13f811e745a88c7e75a3ebc5fead5b4af152a3",
        200303,
        ChainId::Mainnet
    )]
    fn execute_reverted_transaction(hash: &str, block_number: u64, chain: ChainId) {
        let tx_hash = TransactionHash(felt!(hash));
        let block_number = BlockNumber(block_number);

        let full_reader = FullStateReader::new(chain);
        let block_reader = BlockStateReader::new(
            block_number
                .prev()
                .expect("block number should not be zero"),
            &full_reader,
        );
        let mut state = CachedState::new(block_reader);
        let block_context =
            get_block_context(&full_reader, block_number).expect("failed to get block context");

        let flags = ExecutionFlags {
            only_query: false,
            charge_fee: false,
            validate: false,
            strict_nonce_check: true,
        };
        let execution = execute_tx(&mut state, &full_reader, &block_context, tx_hash, flags)
            .expect("failed to execute transaction")
            .result
            .expect("transaction execution failed");

        assert!(execution.is_reverted())
    }
}
