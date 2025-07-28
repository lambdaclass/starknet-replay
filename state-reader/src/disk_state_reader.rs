//! This crate contains logic for caching a node state in disk.
//!
//! The cache is saved to the relative directory `./cache/`:
//! - `./cache/block/`: Contains raw block data.
//! - `./cache/state/`: Contains block state data.
//! - `./cache/contract_class/`: Contains raw contract classes.
//! - `./cache/tx/`: Contains raw transactions.
//! - `./cache/tx_receipt/`: Contains raw transaction receipts.

use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use starknet_api::{
    block::BlockNumber,
    core::{ChainId, ClassHash},
    transaction::{Transaction, TransactionHash},
};
use starknet_core::types::{BlockWithTxHashes, ContractClass};

use crate::{
    cache::BlockState,
    error::StateReaderError,
    objects::RpcTransactionReceipt,
    utils::{read_atomically, write_atomically},
};

/// A disk state reader for network state.
///
/// TODO: To reduce disk usage, we can compress the data before writing it or
/// use a format different from JSON.
#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct DiskStateReader {
    chain_id: ChainId,
}

impl DiskStateReader {
    pub fn new(chain_id: ChainId) -> Self {
        Self { chain_id }
    }

    pub fn get_contract_class(
        &self,
        class_hash: ClassHash,
    ) -> Result<ContractClass, StateReaderError> {
        let cache_path = format!(
            "cache/contract_class/{}.json",
            class_hash.to_fixed_hex_string()
        );
        read_atomically(cache_path)
    }

    pub fn set_contract_class(
        &self,
        class_hash: ClassHash,
        contract_class: &ContractClass,
    ) -> Result<(), StateReaderError> {
        let cache_path = format!(
            "cache/contract_class/{}.json",
            class_hash.to_fixed_hex_string()
        );
        write_atomically(cache_path, contract_class)?;
        Ok(())
    }

    pub fn get_transaction_receipt(
        &self,
        tx_hash: TransactionHash,
    ) -> Result<RpcTransactionReceipt, StateReaderError> {
        let cache_path = format!("cache/tx_receipt/{}.json", tx_hash.to_fixed_hex_string());
        read_atomically(cache_path)
    }

    pub fn set_transaction_receipt(
        &self,
        tx_hash: TransactionHash,
        receipt: &RpcTransactionReceipt,
    ) -> Result<(), StateReaderError> {
        let cache_path = format!("cache/tx_receipt/{}.json", tx_hash.to_fixed_hex_string());
        write_atomically(cache_path, receipt)?;
        Ok(())
    }

    pub fn get_transaction(
        &self,
        tx_hash: TransactionHash,
    ) -> Result<Transaction, StateReaderError> {
        let cache_path = format!("cache/tx/{}.json", tx_hash.to_fixed_hex_string());
        read_atomically(cache_path)
    }

    pub fn set_transaction(
        &self,
        tx_hash: TransactionHash,
        tx: &Transaction,
    ) -> Result<(), StateReaderError> {
        let cache_path = format!("cache/tx/{}.json", tx_hash.to_fixed_hex_string());
        write_atomically(cache_path, tx)?;
        Ok(())
    }

    pub fn get_block(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockWithTxHashes, StateReaderError> {
        let cache_path = format!("cache/block/{}-{}.json", self.chain_id, block_number);
        read_atomically(cache_path)
    }

    pub fn set_block(
        &self,
        block_number: BlockNumber,
        block: &BlockWithTxHashes,
    ) -> Result<(), StateReaderError> {
        let cache_path = format!("cache/block/{}-{}.json", self.chain_id, block_number);
        write_atomically(cache_path, block)?;
        Ok(())
    }

    pub fn get_block_state(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockState, StateReaderError> {
        let cache_path = format!("cache/state/{}-{}.json", self.chain_id, block_number);
        read_atomically(cache_path)
    }

    pub fn set_block_state(
        &self,
        block_number: BlockNumber,
        block_state: &BlockState,
    ) -> Result<(), StateReaderError> {
        let cache_path = format!("cache/state/{}-{}.json", self.chain_id, block_number);
        write_atomically(cache_path, block_state)?;
        Ok(())
    }
}
