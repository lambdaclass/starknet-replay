//! This crate contains the main state reader, which acts
//! as an orchestrator for the other state readers.
//!
//! If the value is cached, it is retrieved directly. If not, it is fetched either
//! from disk or from a starknet node.

use std::cell::{Cell, RefCell, RefMut};
use std::collections::hash_map::Entry;

use crate::cache::{BlockState, StateCache};
use crate::class_manager::ClassManager;
use crate::disk_state_reader::DiskStateReader;
use crate::error::StateReaderError;
use crate::objects::RpcTransactionReceipt;

use crate::remote_state_reader::{url_from_env, RemoteStateReader};
use blockifier::execution::contract_class::RunnableCompiledClass;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use starknet_api::contract_class::compiled_class_hash::{self, HashableCompiledClass};
use starknet_api::core::CompiledClassHash;
use starknet_api::{
    block::BlockNumber,
    contract_class::ClassInfo,
    core::{ChainId, ClassHash, ContractAddress, Nonce},
    state::StorageKey,
    transaction::{Transaction, TransactionHash},
};
use starknet_core::types::{BlockId, BlockWithTxHashes, ContractClass};
use starknet_types_core::felt::Felt;

/// Reader and cache for a Starknet node's state.
///
/// Dropping this reader may take a while, as it has to save data to disk.
///
/// As the blockifier state reader expects an inmutable reference, we need a
/// `RefCell`/`Cell` to allow method signatures to receive inmutable references. As there
/// is no recursion, a runtime panic is impossible.
pub struct FullStateReader {
    chain_id: ChainId,
    hit_counter: Cell<u64>,
    miss_counter: Cell<u64>,
    cache: RefCell<StateCache>,
    disk_reader: DiskStateReader,
    remote_reader: RemoteStateReader,
    class_manager: RefCell<ClassManager>,
}

impl FullStateReader {
    pub fn new(chain_id: ChainId) -> Self {
        let remote_url = url_from_env(&chain_id);
        let remote_reader = RemoteStateReader::new(remote_url);
        Self {
            remote_reader,
            cache: RefCell::new(StateCache::default()),
            disk_reader: DiskStateReader::new(chain_id.clone()),
            class_manager: RefCell::new(ClassManager::new()),
            hit_counter: Cell::new(0),
            miss_counter: Cell::new(0),
            chain_id,
        }
    }

    pub fn reset_counters(&self) {
        self.hit_counter.set(0);
        self.miss_counter.set(0);
        self.remote_reader.reset_counters();
    }

    pub fn get_hit_counter(&self) -> u64 {
        self.hit_counter.get()
    }

    pub fn get_miss_counter(&self) -> u64 {
        self.miss_counter.get()
    }

    pub fn get_rpc_timeout_counter(&self) -> u64 {
        self.remote_reader.get_timeout_counter()
    }

    pub fn get_block(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockWithTxHashes, StateReaderError> {
        // Check in memory cache.
        if let Some(block) = self.cache.borrow().blocks.get(&block_number) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(block.clone());
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        // Try to read from disk cache.
        let block = if let Ok(block) = self.disk_reader.get_block(block_number) {
            block
        } else {
            // If not found, read from remote and save to disk.
            let block = self.remote_reader.get_block_with_tx_hashes(block_number)?;
            self.disk_reader.set_block(block_number, &block)?;
            block
        };

        // Insert into memory cache.
        self.cache
            .borrow_mut()
            .blocks
            .insert(block_number, block.clone());

        Ok(block)
    }

    pub fn get_tx(&self, tx_hash: TransactionHash) -> Result<Transaction, StateReaderError> {
        // Check in memory cache.
        if let Some(tx) = self.cache.borrow().transactions.get(&tx_hash) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(tx.clone());
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        // Try to read from disk cache.
        let tx = if let Ok(tx) = self.disk_reader.get_transaction(tx_hash) {
            tx
        } else {
            // If not found, read from remote and save to disk.
            let tx = self.remote_reader.get_tx(&tx_hash)?;
            self.disk_reader.set_transaction(tx_hash, &tx)?;
            tx
        };

        // Insert into memory cache.
        self.cache
            .borrow_mut()
            .transactions
            .insert(tx_hash, tx.clone());

        Ok(tx)
    }

    pub fn get_tx_receipt(
        &self,
        tx_hash: TransactionHash,
    ) -> Result<RpcTransactionReceipt, StateReaderError> {
        // Check in memory cache.
        if let Some(tx) = self.cache.borrow().transaction_receipts.get(&tx_hash) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(tx.clone());
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        // Try to read from disk cache.
        let receipt = if let Ok(receipt) = self.disk_reader.get_transaction_receipt(tx_hash) {
            receipt
        } else {
            // If not found, read from remote and save to disk.
            let receipt = self.remote_reader.get_tx_receipt(&tx_hash)?;
            self.disk_reader
                .set_transaction_receipt(tx_hash, &receipt)?;
            receipt
        };

        // Insert into memory cache.
        self.cache
            .borrow_mut()
            .transaction_receipts
            .insert(tx_hash, receipt.clone());

        Ok(receipt)
    }

    pub fn get_storage_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> Result<Felt, StateReaderError> {
        let mut cache = self.cache.borrow_mut();

        // Find the block state cache.
        let block_state = self.get_block_state(&mut cache, block_number);

        // Check in memory cache.
        if let Some(value) = block_state.storage.get(&(contract_address, key)) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(*value);
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        // If not found, read from remote and save to cache.
        let value = self
            .remote_reader
            .get_storage_at(block_number, contract_address, key)?;
        block_state.storage.insert((contract_address, key), value);

        Ok(value)
    }

    pub fn get_nonce_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> Result<Nonce, StateReaderError> {
        let mut cache = self.cache.borrow_mut();

        // Find the block state cache.
        let block_state = self.get_block_state(&mut cache, block_number);

        // Check in memory cache.
        if let Some(nonce) = block_state.nonces.get(&contract_address) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(*nonce);
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        // If not found, read from remote and save to cache.
        let nonce = self
            .remote_reader
            .get_nonce_at(block_number, contract_address)?;
        block_state.nonces.insert(contract_address, nonce);

        Ok(nonce)
    }

    pub fn get_class_hash_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> Result<ClassHash, StateReaderError> {
        let mut cache = self.cache.borrow_mut();

        // Find the block state cache.
        let block_state = self.get_block_state(&mut cache, block_number);

        // Check in memory cache.
        if let Some(class_hash) = block_state.class_hashes.get(&contract_address) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(*class_hash);
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        // If not found, read from remote and save to cache.
        let class_hash = self
            .remote_reader
            .get_class_hash_at(block_number, contract_address)?;
        block_state
            .class_hashes
            .insert(contract_address, class_hash);

        Ok(class_hash)
    }

    pub fn get_contract_class(
        &self,
        block_id: BlockId,
        class_hash: ClassHash,
    ) -> Result<ContractClass, StateReaderError> {
        // Check in memory cache.
        if let Some(class) = self.cache.borrow().contract_classes.get(&class_hash) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(class.clone());
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        // Try to read from disk cache.
        let class = if let Ok(class) = self.disk_reader.get_contract_class(class_hash) {
            class
        } else {
            // If not found, read from remote and save to disk.
            let class = self
                .remote_reader
                .get_contract_class(block_id, &class_hash)?;
            self.disk_reader.set_contract_class(class_hash, &class)?;
            class
        };

        // Insert into memory cache.
        self.cache
            .borrow_mut()
            .contract_classes
            .insert(class_hash, class.clone());

        Ok(class)
    }

    pub fn get_compiled_class(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<RunnableCompiledClass, StateReaderError> {
        // Check in memory cache.
        if let Some(result) = self.class_manager.borrow().get_runnable_class(&class_hash) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(result.clone());
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        // If not found, compile and cache it.
        let contract_class =
            self.get_contract_class(BlockId::Number(block_number.0), class_hash)?;
        self.class_manager
            .borrow_mut()
            .compile_runnable_class(&class_hash, contract_class)
    }

    pub fn get_compiled_class_hash(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<CompiledClassHash, StateReaderError> {
        let class = self.get_compiled_class(block_number, class_hash)?;

        let compiled_class_hash = match class {
            RunnableCompiledClass::V0(_) => CompiledClassHash::default(),
            RunnableCompiledClass::V1(class) => class.hash(&compiled_class_hash::HashVersion::V1),
            RunnableCompiledClass::V1Native(class) => {
                class.hash(&compiled_class_hash::HashVersion::V1)
            }
        };

        Ok(compiled_class_hash)
    }

    pub fn get_compiled_class_hash_v2(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<CompiledClassHash, StateReaderError> {
        let class = self.get_compiled_class(block_number, class_hash)?;

        let compiled_class_hash = match class {
            RunnableCompiledClass::V0(_) => CompiledClassHash::default(),
            RunnableCompiledClass::V1(class) => class.hash(&compiled_class_hash::HashVersion::V2),
            RunnableCompiledClass::V1Native(class) => {
                class.hash(&compiled_class_hash::HashVersion::V2)
            }
        };

        Ok(compiled_class_hash)
    }

    pub fn get_class_info(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<ClassInfo, StateReaderError> {
        let contract_class =
            self.get_contract_class(BlockId::Number(block_number.0), class_hash)?;

        // This value is not cached in memory as its only used before executing
        // a transaction. This means that it won't affect a transaction
        // performance.
        self.class_manager
            .borrow()
            .get_class_info(&class_hash, contract_class)
    }

    pub fn get_chain_id(&self) -> Result<ChainId, StateReaderError> {
        Ok(self.chain_id.clone())
    }

    pub fn get_block_state<'c>(
        &self,
        cache: &'c mut RefMut<StateCache>,
        block_number: BlockNumber,
    ) -> &'c mut BlockState {
        match cache.block_states.entry(block_number) {
            Entry::Occupied(occupied_entry) => {
                self.hit_counter.set(self.hit_counter.get() + 1);
                occupied_entry.into_mut()
            }
            Entry::Vacant(vacant_entry) => {
                // If not found, read state cache from disk.
                self.miss_counter.set(self.miss_counter.get() + 1);
                let block_state = self
                    .disk_reader
                    .get_block_state(block_number)
                    .unwrap_or_default();
                vacant_entry.insert_entry(block_state).into_mut()
            }
        }
    }
}

/// Before dropping the full state reader,
/// we save the cached block states to disk.
impl Drop for FullStateReader {
    fn drop(&mut self) {
        self.cache
            .borrow()
            .block_states
            .par_iter()
            .for_each(|(block_number, block_state)| {
                let _ = self.disk_reader.set_block_state(*block_number, block_state);
            });
    }
}

#[cfg(test)]
mod tests {
    use starknet_api::{
        block::BlockNumber,
        class_hash, contract_address,
        core::ChainId,
        felt, storage_key,
        transaction::{TransactionHash, TransactionVersion},
    };

    use crate::full_state_reader::FullStateReader;

    #[test]
    pub fn get_contract_class() {
        let state = FullStateReader::new(ChainId::Mainnet);

        state
            .get_compiled_class(
                BlockNumber(1500000),
                class_hash!("0x07f3331378862ed0a10f8c3d49f4650eb845af48f1c8120591a43da8f6f12679"),
            )
            .unwrap();
        state
            .get_compiled_class(
                BlockNumber(1500000),
                class_hash!("0x07f3331378862ed0a10f8c3d49f4650eb845af48f1c8120591a43da8f6f12679"),
            )
            .unwrap();
        assert_eq!(state.get_miss_counter(), 2);
        assert_eq!(state.get_hit_counter(), 1);
    }

    #[test]
    pub fn get_legacy_contract_class() {
        let state = FullStateReader::new(ChainId::Mainnet);

        state
            .get_compiled_class(
                BlockNumber(1500000),
                class_hash!("0x010455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8"),
            )
            .unwrap();

        state
            .get_compiled_class(
                BlockNumber(1500000),
                class_hash!("0x010455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8"),
            )
            .unwrap();
        assert_eq!(state.get_miss_counter(), 2);
        assert_eq!(state.get_hit_counter(), 1);
    }

    #[test]
    pub fn get_contract_class_info() {
        let state = FullStateReader::new(ChainId::Mainnet);

        state
            .get_class_info(
                BlockNumber(1500000),
                class_hash!("0x07f3331378862ed0a10f8c3d49f4650eb845af48f1c8120591a43da8f6f12679"),
            )
            .unwrap();
        state
            .get_class_info(
                BlockNumber(1500000),
                class_hash!("0x07f3331378862ed0a10f8c3d49f4650eb845af48f1c8120591a43da8f6f12679"),
            )
            .unwrap();
        assert_eq!(state.get_miss_counter(), 1);
        assert_eq!(state.get_hit_counter(), 1);
    }

    #[test]
    pub fn get_legacy_contract_class_info() {
        let state = FullStateReader::new(ChainId::Mainnet);

        state
            .get_class_info(
                BlockNumber(1500000),
                class_hash!("0x010455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8"),
            )
            .unwrap();
        state
            .get_class_info(
                BlockNumber(1500000),
                class_hash!("0x010455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8"),
            )
            .unwrap();
        assert_eq!(state.get_miss_counter(), 1);
        assert_eq!(state.get_hit_counter(), 1);
    }

    #[test]
    pub fn get_cached_storage() {
        let state = FullStateReader::new(ChainId::Mainnet);

        let value = state
            .get_storage_at(
                BlockNumber(1500000),
                contract_address!(
                    "0x055e557a4c975059522a1321d7a7bd215287450907419e5f8aa98145c7699a2c"
                ),
                storage_key!("0x01ccc09c8a19948e048de7add6929589945e25f22059c7345aaf7837188d8d05"),
            )
            .unwrap();

        assert_eq!(
            value,
            felt!("0x4088b3713e2753e7801f4ba098a8afd879ae5c7a167bbaefdc750e1040cfa48")
        );

        state.reset_counters();

        let value = state
            .get_storage_at(
                BlockNumber(1500000),
                contract_address!(
                    "0x055e557a4c975059522a1321d7a7bd215287450907419e5f8aa98145c7699a2c"
                ),
                storage_key!("0x01ccc09c8a19948e048de7add6929589945e25f22059c7345aaf7837188d8d05"),
            )
            .unwrap();

        assert_eq!(
            value,
            felt!("0x4088b3713e2753e7801f4ba098a8afd879ae5c7a167bbaefdc750e1040cfa48")
        );
        assert_eq!(state.get_hit_counter(), 2);
    }

    #[test]
    pub fn get_disk_cached_storage() {
        let state = FullStateReader::new(ChainId::Mainnet);

        let value = state
            .get_storage_at(
                BlockNumber(1500000),
                contract_address!(
                    "0x055e557a4c975059522a1321d7a7bd215287450907419e5f8aa98145c7699a2c"
                ),
                storage_key!("0x01ccc09c8a19948e048de7add6929589945e25f22059c7345aaf7837188d8d05"),
            )
            .unwrap();

        assert_eq!(
            value,
            felt!("0x4088b3713e2753e7801f4ba098a8afd879ae5c7a167bbaefdc750e1040cfa48")
        );

        drop(state);
        let state = FullStateReader::new(ChainId::Mainnet);

        let value = state
            .get_storage_at(
                BlockNumber(1500000),
                contract_address!(
                    "0x055e557a4c975059522a1321d7a7bd215287450907419e5f8aa98145c7699a2c"
                ),
                storage_key!("0x01ccc09c8a19948e048de7add6929589945e25f22059c7345aaf7837188d8d05"),
            )
            .unwrap();

        assert_eq!(
            value,
            felt!("0x4088b3713e2753e7801f4ba098a8afd879ae5c7a167bbaefdc750e1040cfa48")
        );

        assert_eq!(state.get_miss_counter(), 1);
        assert_eq!(state.get_hit_counter(), 1);
    }

    #[test]
    pub fn get_block() {
        let state = FullStateReader::new(ChainId::Sepolia);

        let block = state.get_block(BlockNumber(750000)).unwrap();

        assert_eq!(block.transactions.len(), 10);
        assert_eq!(state.get_miss_counter(), 1);
        assert_eq!(state.get_hit_counter(), 0);
    }

    #[test]
    pub fn get_block_cached() {
        let state = FullStateReader::new(ChainId::Sepolia);

        let block = state.get_block(BlockNumber(750000)).unwrap();
        assert_eq!(block.transactions.len(), 10);
        assert_eq!(state.get_miss_counter(), 1);
        assert_eq!(state.get_hit_counter(), 0);

        let block = state.get_block(BlockNumber(750000)).unwrap();
        assert_eq!(block.transactions.len(), 10);
        assert_eq!(state.get_miss_counter(), 1);
        assert_eq!(state.get_hit_counter(), 1);
    }

    #[test]
    pub fn get_tx() {
        let state = FullStateReader::new(ChainId::Sepolia);

        let tx = state
            .get_tx(TransactionHash(felt!(
                "0x186f6c7338057937dfca8f6feb85dfa056a46d496b75659bc7145d15e4c25ed"
            )))
            .unwrap();

        assert_eq!(tx.version(), TransactionVersion(felt!("0x3")));
        assert_eq!(state.get_miss_counter(), 1);
        assert_eq!(state.get_hit_counter(), 0);

        let tx = state
            .get_tx(TransactionHash(felt!(
                "0x186f6c7338057937dfca8f6feb85dfa056a46d496b75659bc7145d15e4c25ed"
            )))
            .unwrap();

        assert_eq!(tx.version(), TransactionVersion(felt!("0x3")));
        assert_eq!(state.get_miss_counter(), 1);
        assert_eq!(state.get_hit_counter(), 1);
    }
}
