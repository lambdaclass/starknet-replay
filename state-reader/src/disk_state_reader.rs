use std::collections::HashMap;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use starknet_api::{
    block::BlockNumber,
    core::{ChainId, ClassHash, ContractAddress, Nonce},
    state::StorageKey,
    transaction::{Transaction, TransactionHash},
};
use starknet_core::types::{BlockWithTxHashes, ContractClass};
use starknet_types_core::felt::Felt;

use crate::{
    error::StateReaderError,
    objects::RpcTransactionReceipt,
    utils::{merge_atomically, read_atomically, write_atomically},
};

/// A disk state reader for network state.
///
/// Also keeps items in memory to avoid querying the file system often.
///
/// TODO: To reduce disk usage, we can compress the data before writing it or
/// use a format different from JSON.
#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct DiskStateReader {
    chain_id: ChainId,
    blocks: HashMap<BlockNumber, BlockWithTxHashes>,
    transactions: HashMap<TransactionHash, Transaction>,
    transaction_receipts: HashMap<TransactionHash, RpcTransactionReceipt>,
    contract_classes: HashMap<ClassHash, ContractClass>,
    block_state_caches: HashMap<BlockNumber, BlockStateCache>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct BlockStateCache {
    #[serde_as(as = "Vec<(_, _)>")]
    pub nonces: HashMap<ContractAddress, Nonce>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub class_hashes: HashMap<ContractAddress, ClassHash>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub storage: HashMap<(ContractAddress, StorageKey), Felt>,
}

impl DiskStateReader {
    pub fn new(chain_id: ChainId) -> Self {
        Self {
            chain_id,
            blocks: Default::default(),
            transactions: Default::default(),
            transaction_receipts: Default::default(),
            contract_classes: Default::default(),
            block_state_caches: Default::default(),
        }
    }

    pub fn save(&self) -> Result<(), StateReaderError> {
        let results = self
            .block_state_caches
            .par_iter()
            .map(|(block_number, block_cache)| {
                let cache_path = format!("cache/state/{}-{}.json", self.chain_id, block_number);
                merge_atomically(cache_path, block_cache.clone(), merge_state_cache)
            })
            .collect::<Vec<_>>();
        results.into_iter().collect::<Result<Vec<_>, _>>()?;
        Ok(())
    }

    pub fn get_contract_class(
        &mut self,
        class_hash: ClassHash,
    ) -> Result<ContractClass, StateReaderError> {
        if let Some(contract_class) = self.contract_classes.get(&class_hash) {
            return Ok(contract_class.clone());
        }

        let cache_path = format!(
            "cache/contract_class/{}.json",
            class_hash.to_fixed_hex_string()
        );
        let contract_class = read_atomically(cache_path)?;

        Ok(contract_class)
    }

    pub fn set_contract_class(
        &mut self,
        class_hash: ClassHash,
        contract_class: ContractClass,
    ) -> Result<(), StateReaderError> {
        let cache_path = format!(
            "cache/contract_class/{}.json",
            class_hash.to_fixed_hex_string()
        );
        write_atomically(cache_path, &contract_class)?;

        self.contract_classes.insert(class_hash, contract_class);

        Ok(())
    }

    pub fn get_transaction_receipt(
        &mut self,
        tx_hash: TransactionHash,
    ) -> Result<RpcTransactionReceipt, StateReaderError> {
        if let Some(receipt) = self.transaction_receipts.get(&tx_hash) {
            return Ok(receipt.clone());
        }

        let cache_path = format!("cache/tx_receipt/{}.json", tx_hash.to_fixed_hex_string());
        let receipt = read_atomically(cache_path)?;

        Ok(receipt)
    }

    pub fn set_transaction_receipt(
        &mut self,
        tx_hash: TransactionHash,
        receipt: RpcTransactionReceipt,
    ) -> Result<(), StateReaderError> {
        let cache_path = format!("cache/tx_receipt/{}.json", tx_hash.to_fixed_hex_string());
        write_atomically(cache_path, &receipt)?;

        self.transaction_receipts.insert(tx_hash, receipt);

        Ok(())
    }

    pub fn get_transaction(
        &mut self,
        tx_hash: TransactionHash,
    ) -> Result<Transaction, StateReaderError> {
        if let Some(receipt) = self.transactions.get(&tx_hash) {
            return Ok(receipt.clone());
        }

        let cache_path = format!("cache/tx/{}.json", tx_hash.to_fixed_hex_string());
        let receipt = read_atomically(cache_path)?;

        Ok(receipt)
    }

    pub fn set_transaction(
        &mut self,
        tx_hash: TransactionHash,
        tx: Transaction,
    ) -> Result<(), StateReaderError> {
        let cache_path = format!("cache/tx/{}.json", tx_hash.to_fixed_hex_string());
        write_atomically(cache_path, &tx)?;

        self.transactions.insert(tx_hash, tx);

        Ok(())
    }

    pub fn get_block(
        &mut self,
        block_number: BlockNumber,
    ) -> Result<BlockWithTxHashes, StateReaderError> {
        if let Some(receipt) = self.blocks.get(&block_number) {
            return Ok(receipt.clone());
        }

        let cache_path = format!("cache/block/{}-{}.json", self.chain_id, block_number);
        let receipt = read_atomically(cache_path)?;

        Ok(receipt)
    }

    pub fn set_block(
        &mut self,
        block_number: BlockNumber,
        block: BlockWithTxHashes,
    ) -> Result<(), StateReaderError> {
        let cache_path = format!("cache/block/{}-{}.json", self.chain_id, block_number);
        write_atomically(cache_path, &block)?;

        self.blocks.insert(block_number, block);

        Ok(())
    }

    pub fn get_block_state_cache(
        &mut self,
        block_number: BlockNumber,
    ) -> Result<&mut BlockStateCache, StateReaderError> {
        if !self.block_state_caches.contains_key(&block_number) {
            let cache_path = format!("cache/state/{}-{}.json", self.chain_id, block_number);
            let block_state_cache = read_atomically(cache_path).unwrap_or_default();
            let _ = self
                .block_state_caches
                .insert(block_number, block_state_cache);
        }

        Ok(self
            .block_state_caches
            .get_mut(&block_number)
            .expect("just inserted"))
    }
}

pub fn merge_state_cache(mut v1: BlockStateCache, v2: BlockStateCache) -> BlockStateCache {
    v2.nonces.into_iter().for_each(|(k, v)| {
        let old = v1.nonces.insert(k, v);
        if let Some(old) = old {
            assert_eq!(old, v)
        }
    });
    v2.class_hashes.into_iter().for_each(|(k, v)| {
        let old = v1.class_hashes.insert(k, v);
        if let Some(old) = old {
            assert_eq!(old, v)
        }
    });
    v2.storage.into_iter().for_each(|(k, v)| {
        let old = v1.storage.insert(k, v);
        if let Some(old) = old {
            assert_eq!(old, v)
        }
    });

    v1
}
