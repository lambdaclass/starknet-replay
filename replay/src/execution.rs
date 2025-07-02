use blockifier::{
    blockifier::block::validated_gas_prices,
    bouncer::BouncerConfig,
    context::BlockContext,
    state::{cached_state::CachedState, state_api::StateReader},
    transaction::{
        account_transaction::ExecutionFlags,
        transaction_execution::Transaction as BlockifierTransaction,
        transactions::ExecutableTransaction,
    },
    versioned_constants::VersionedConstants,
};
use blockifier_reexecution::state_reader::utils::get_chain_info;
use starknet_api::{
    block::{BlockInfo, BlockNumber, BlockTimestamp, GasPrice, NonzeroGasPrice, StarknetVersion},
    core::{ContractAddress, PatriciaKey},
    test_utils::MAX_FEE,
    transaction::{Transaction, TransactionHash},
};
use starknet_core::types::{BlockWithTxHashes, Felt};
use state_reader::{block_state_reader::BlockStateReader, full_state_reader::FullStateReader};
use tracing::{error, info, info_span};

pub fn execute_block(
    reader: &FullStateReader,
    block_number: BlockNumber,
    execution_flags: ExecutionFlags,
) -> anyhow::Result<()> {
    let _block_execution_span = info_span!("block execution", block = block_number.0).entered();

    let block = reader.get_block(block_number)?;
    let tx_hashes: Vec<TransactionHash> = block
        .transactions
        .into_iter()
        .map(TransactionHash)
        .collect();

    execute_txs(reader, block_number, tx_hashes, execution_flags)?;

    Ok(())
}

pub fn execute_txs(
    reader: &FullStateReader,
    block_number: BlockNumber,
    tx_hashes: Vec<TransactionHash>,
    execution_flags: ExecutionFlags,
) -> anyhow::Result<()> {
    let block_reader = BlockStateReader::new(
        block_number
            .prev()
            .expect("block number should not be zero"),
        reader,
    );
    let mut state = CachedState::new(block_reader);
    let block_context = get_block_context(reader, block_number)?;

    for tx_hash in tx_hashes {
        let _ = execute_tx(
            &mut state,
            reader,
            &block_context,
            tx_hash,
            execution_flags.clone(),
        );
    }

    Ok(())
}
pub fn execute_tx(
    state: &mut CachedState<impl StateReader>,
    reader: &FullStateReader,
    block_context: &BlockContext,
    tx_hash: TransactionHash,
    execution_flags: ExecutionFlags,
) -> anyhow::Result<()> {
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

    let execution_info_result = tx.execute(state, block_context);

    info!("finished transaction execution");

    #[cfg(feature = "state_dump")]
    crate::state_dump::create_state_dump(
        state,
        block_context.block_info().block_number.0,
        &tx_hash.to_hex_string(),
        &execution_info_result,
    );

    #[cfg(feature = "with-libfunc-profiling")]
    crate::libfunc_profile::create_libfunc_profile(tx_hash.to_hex_string());

    execution_info_result.inspect_err(|err| error!("failed to execute transaction: {}", err))?;

    Ok(())
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
