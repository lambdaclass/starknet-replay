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
        if let Ok(execution) = execute_tx(
            &mut state,
            reader,
            &block_context,
            tx_hash,
            execution_flags.clone(),
        )
        .inspect_err(|err| error!("failed to execute transaction: {}", err))
        {
            executions.push(execution);
        }
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
    )?;

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
pub fn validate_tx_with_receipt(
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
    use std::{fs::File, time::Duration};

    use blockifier::{
        execution::call_info::CallInfo,
        state::cached_state::CachedState,
        transaction::{account_transaction::ExecutionFlags, objects::TransactionExecutionInfo},
    };
    use pretty_assertions_sorted::assert_eq_sorted;
    use starknet_api::{block::BlockNumber, core::ChainId, felt, transaction::TransactionHash};
    use state_reader::{block_state_reader::BlockStateReader, full_state_reader::FullStateReader};
    use test_case::test_case;

    use crate::execution::{execute_tx, get_block_context};

    fn normalize_execution_info(execution_info: &mut TransactionExecutionInfo) {
        fn normalize_call_info(call_info: &mut CallInfo) {
            call_info.time = Duration::default();
            call_info.execution.cairo_native = false;
            call_info
                .inner_calls
                .iter_mut()
                .for_each(normalize_call_info);
        }
        execution_info
            .validate_call_info
            .as_mut()
            .map(normalize_call_info);
        execution_info
            .execute_call_info
            .as_mut()
            .map(normalize_call_info);
        execution_info
            .fee_transfer_call_info
            .as_mut()
            .map(normalize_call_info);
    }

    #[test_case(
        "0x00164bfc80755f62de97ae7c98c9d67c1767259427bcf4ccfcc9683d44d54676",
        197001,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x00724fc4a84f489ed032ebccebfc9541eb8dc64b0e76b933ed6fc30cd6000bd1",
        186552,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x00f390691fd9e865f5aef9c7cc99889fb6c2038bc9b7e270e8a4fe224ccd404d",
        662251,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x014640564509873cf9d24a311e1207040c8b60efd38d96caef79855f0b0075d5",
        90007,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x025844447697eb7d5df4d8268b23aef6c11de4087936048278c2559fc35549eb",
        197001,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x026c17728b9cd08a061b1f17f08034eb70df58c1a96421e73ee6738ad258a94c",
        169929,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x026e04e96ba1b75bfd066c8e138e17717ecb654909e6ac24007b644ac23e4b47",
        536893,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x0310c46edc795c82c71f600159fa9e6c6540cb294df9d156f685bfe62b31a5f4",
        662249,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x0355059efee7a38ba1fd5aef13d261914608dce7bdfacad92a71e396f0ad7a77",
        661815,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x04756d898323a8f884f5a6aabd6834677f4bbaeecc2522f18b3ae45b3f99cd1e",
        662250,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x04ba569a40a866fd1cbb2f3d3ba37ef68fb91267a4931a377d6acc6e5a854f9a",
        648462,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x04db9b88e07340d18d53b8b876f28f449f77526224afb372daaf1023c8b08036",
        398052,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x04df8a364233d995c33c7f4666a776bf458631bec2633e932b433a783db410f8",
        422882,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x0528ec457cf8757f3eefdf3f0728ed09feeecc50fd97b1e4c5da94e27e9aa1d6",
        169929,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x05324bac55fb9fb53e738195c2dcc1e7fed1334b6db824665e3e984293bec95e",
        662246,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x05d200ef175ba15d676a68b36f7a7b72c17c17604eda4c1efc2ed5e4973e2c91",
        169929,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x066e1f01420d8e433f6ef64309adb1a830e5af0ea67e3d935de273ca57b3ae5e",
        662252,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x06962f11a96849ebf05cd222313858a93a8c5f300493ed6c5859dd44f5f2b4e3",
        654770,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x06a09ffbf996178ac6e90101047e42fe29cb7108573b2ecf4b0ebd2cba544cb4",
        662248,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x0737677385a30ec4cbf9f6d23e74479926975b74db3d55dc5e46f4f8efee41cf",
        169929,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x0743092843086fa6d7f4a296a226ee23766b8acf16728aef7195ce5414dc4d84",
        186549,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x0780e3a498b4fd91ab458673891d3e8ee1453f9161f4bfcb93dd1e2c91c52e10",
        650558,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x078b81326882ecd2dc6c5f844527c3f33e0cdb52701ded7b1aa4d220c5264f72",
        653019,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x176a92e8df0128d47f24eebc17174363457a956fa233cc6a7f8561bfbd5023a",
        317093,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x1ecb4b825f629eeb9816ddfd6905a85f6d2c89995907eacaf6dc64e27a2c917",
        654001,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x26be3e906db66973de1ca5eec1ddb4f30e3087dbdce9560778937071c3d3a83",
        351269,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x2d2bed435d0b43a820443aad2bc9e3d4fa110c428e65e422101dfa100ba5664",
        653001,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x41497e62fb6798ff66e4ad736121c0164cdb74005aa5dab025be3d90ad4ba06",
        638867,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x4f552c9430bd21ad300db56c8f4cae45d554a18fac20bf1703f180fac587d7e",
        351226,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x5a5de1f42f6005f3511ea6099daed9bcbcf9de334ee714e8563977e25f71601",
        281514,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x670321c71835004fcab639e871ef402bb807351d126ccc4d93075ff2c31519d",
        654001,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x70d83cb9e25f1e9f7be2608f72c7000796e4a222c1ed79a0ea81abe5172557b",
        654001,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x73ef9cde09f005ff6f411de510ecad4cdcf6c4d0dfc59137cff34a4fc74dfd",
        654001,
        ChainId::Mainnet
    )]
    #[test_case(
        "0x75d7ef42a815e4d9442efcb509baa2035c78ea6a6272ae29e87885788d4c85e",
        654001,
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
        let mut execution = execute_tx(&mut state, &full_reader, &block_context, tx_hash, flags)
            .expect("failed to execute transaction")
            .result
            .expect("transaction execution failed");

        normalize_execution_info(&mut execution);
        let summary = execution.summarize(block_context.versioned_constants());

        let expected_summary_file =
            File::open(format!("../test_data/execution_summary/{}.json", hash)).unwrap();
        let expected_summary = serde_json::from_reader(expected_summary_file).unwrap();
        assert_eq_sorted!(summary, expected_summary);

        assert!(
            !execution.is_reverted(),
            "{}",
            execution.revert_error.unwrap()
        )
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
