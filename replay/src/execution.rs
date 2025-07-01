use blockifier::{
    blockifier::block::validated_gas_prices,
    bouncer::BouncerConfig,
    context::BlockContext,
    transaction::{
        account_transaction::ExecutionFlags,
        transaction_execution::Transaction as BlockifierTransaction,
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
use state_reader::StateManager;

pub fn get_block_context(
    state_manager: &StateManager,
    block_number: BlockNumber,
) -> anyhow::Result<BlockContext> {
    let block = state_manager.get_block(block_number)?;
    let chain_id = state_manager.get_chain_id()?;

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
    state_manager: &StateManager,
    block_number: BlockNumber,
    tx_hash: TransactionHash,
    execution_flags: ExecutionFlags,
) -> anyhow::Result<BlockifierTransaction> {
    let tx = state_manager.get_tx(tx_hash)?;

    let class_info = if let Transaction::Declare(declare_tx) = &tx {
        Some(state_manager.get_class_info(block_number, declare_tx.class_hash())?)
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
