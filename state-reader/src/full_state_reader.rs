use std::cell::{Cell, RefCell};

use crate::class_manager::{ClassManager, ClassManagerError};
use crate::objects::RpcTransactionReceipt;
use crate::remote_state_reader::{url_from_env, RemoteStateReader, RemoteStateReaderError};
use starknet_api::{
    block::BlockNumber,
    contract_class::ClassInfo,
    core::{ChainId, ClassHash, ContractAddress, Nonce},
    state::StorageKey,
    transaction::{Transaction, TransactionHash},
};

use starknet_core::types::{BlockWithTxHashes, ContractClass};
use starknet_types_core::felt::Felt;

use crate::state_cache::{StateCache, StateCacheError};
use blockifier::execution::contract_class::RunnableCompiledClass;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FullStateReaderError {
    #[error(transparent)]
    RemoteReaderError(#[from] RemoteStateReaderError),
    #[error(transparent)]
    ClassManagerError(#[from] ClassManagerError),
    #[error(transparent)]
    StateCacheError(#[from] StateCacheError),
}

/// Reader and cache for a Starknet node's state.
///
/// Creating/Dropping this reader may take a while, as it has to load/save data to disk.
///
/// As the blockifier state reader expectes an inmutable reference, we need a
/// `RefCell`/`Cell` allow method signatures to receive inmutable references. As there
/// is no recursion, a runtime panic is impossible.
pub struct FullStateReader {
    hit_counter: Cell<u64>,
    miss_counter: Cell<u64>,
    remote_reader: RemoteStateReader,
    cache: RefCell<StateCache>,
    class_manager: RefCell<ClassManager>,
}

impl FullStateReader {
    pub fn load(chain_id: ChainId) -> Result<Self, FullStateReaderError> {
        let remote_url = url_from_env(&chain_id);
        let remote_reader = RemoteStateReader::new(remote_url);
        Ok(Self {
            remote_reader,
            cache: RefCell::new(StateCache::load(chain_id)?),
            class_manager: RefCell::new(ClassManager::new()),
            hit_counter: Cell::new(0),
            miss_counter: Cell::new(0),
        })
    }

    pub fn new(chain_id: ChainId) -> Self {
        let remote_url = url_from_env(&chain_id);
        let remote_reader = RemoteStateReader::new(remote_url);
        Self {
            remote_reader,
            cache: RefCell::new(StateCache::new(chain_id)),
            class_manager: RefCell::new(ClassManager::new()),
            hit_counter: Cell::new(0),
            miss_counter: Cell::new(0),
        }
    }

    pub fn reset_counters(&self) {
        self.hit_counter.set(0);
        self.miss_counter.set(0);
    }

    pub fn get_hit_counter(&self) -> u64 {
        self.hit_counter.get()
    }

    pub fn get_miss_counter(&self) -> u64 {
        self.miss_counter.get()
    }

    pub fn get_block(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockWithTxHashes, FullStateReaderError> {
        if let Some(result) = self.cache.borrow().blocks.get(&block_number) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(result.clone());
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        let result = self.remote_reader.get_block_with_tx_hashes(block_number)?;

        self.cache
            .borrow_mut()
            .blocks
            .insert(block_number, result.clone());

        Ok(result)
    }

    pub fn get_tx(&self, tx_hash: TransactionHash) -> Result<Transaction, FullStateReaderError> {
        if let Some(result) = self.cache.borrow().transactions.get(&tx_hash) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(result.clone());
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        let result = self.remote_reader.get_tx(&tx_hash)?;

        self.cache
            .borrow_mut()
            .transactions
            .insert(tx_hash, result.clone());

        Ok(result)
    }

    pub fn get_tx_receipt(
        &self,
        tx_hash: TransactionHash,
    ) -> Result<RpcTransactionReceipt, FullStateReaderError> {
        if let Some(result) = self.cache.borrow().transaction_receipts.get(&tx_hash) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(result.clone());
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        let result = self.remote_reader.get_tx_receipt(&tx_hash)?;
        self.cache
            .borrow_mut()
            .transaction_receipts
            .insert(tx_hash, result.clone());

        Ok(result)
    }

    pub fn get_storage_at(
        &self,
        block_number: BlockNumber,

        contract_address: ContractAddress,
        key: StorageKey,
    ) -> Result<Felt, FullStateReaderError> {
        if let Some(result) =
            self.cache
                .borrow()
                .storage
                .get(&(block_number, contract_address, key))
        {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(*result);
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        let result = self
            .remote_reader
            .get_storage_at(block_number, contract_address, key)?;

        self.cache
            .borrow_mut()
            .storage
            .insert((block_number, contract_address, key), result);

        Ok(result)
    }

    pub fn get_nonce_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> Result<Nonce, FullStateReaderError> {
        if let Some(result) = self
            .cache
            .borrow()
            .nonces
            .get(&(block_number, contract_address))
        {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(*result);
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        let result = self
            .remote_reader
            .get_nonce_at(block_number, contract_address)?;

        self.cache
            .borrow_mut()
            .nonces
            .insert((block_number, contract_address), result);

        Ok(result)
    }

    pub fn get_class_hash_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> Result<ClassHash, FullStateReaderError> {
        if let Some(result) = self
            .cache
            .borrow()
            .class_hashes
            .get(&(block_number, contract_address))
        {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(*result);
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        let result = self
            .remote_reader
            .get_class_hash_at(block_number, contract_address)?;

        self.cache
            .borrow_mut()
            .class_hashes
            .insert((block_number, contract_address), result);

        Ok(result)
    }

    pub fn get_contract_class(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<ContractClass, FullStateReaderError> {
        if let Some(result) = self.cache.borrow().contract_classes.get(&class_hash) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(result.clone());
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        let result = self
            .remote_reader
            .get_contract_class(block_number, &class_hash)?;

        self.cache
            .borrow_mut()
            .contract_classes
            .insert(class_hash, result.clone());

        Ok(result)
    }

    pub fn get_compiled_class(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<RunnableCompiledClass, FullStateReaderError> {
        if let Some(result) = self.class_manager.borrow().get_runnable_class(&class_hash) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(result.clone());
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        let contract_class = self.get_contract_class(block_number, class_hash)?;

        Ok(self
            .class_manager
            .borrow_mut()
            .compile_runnable_class(&class_hash, contract_class)?)
    }

    pub fn get_class_info(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<ClassInfo, FullStateReaderError> {
        let contract_class = self.get_contract_class(block_number, class_hash)?;

        // This value is not cached in memory as its only used before executing
        // a transaction. This means that it won't affect a transaction
        // performance.
        Ok(self
            .class_manager
            .borrow()
            .get_class_info(&class_hash, contract_class)?)
    }

    pub fn get_chain_id(&self) -> Result<ChainId, FullStateReaderError> {
        Ok(self.cache.borrow().chain_id.clone())
    }
}

impl Drop for FullStateReader {
    fn drop(&mut self) {
        let _ = self.cache.borrow_mut().save();
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

        assert_eq!(state.get_hit_counter(), 1);
        assert_eq!(state.get_miss_counter(), 2);
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

        assert_eq!(state.get_hit_counter(), 1);
        assert_eq!(state.get_miss_counter(), 2);
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

        assert_eq!(state.get_hit_counter(), 1);
        assert_eq!(state.get_miss_counter(), 1);
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
        let state = FullStateReader::load(ChainId::Mainnet).expect("failed to load reader");

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

        assert_eq!(state.get_miss_counter(), 0)
    }

    #[test]
    pub fn get_block() {
        let state = FullStateReader::new(ChainId::Sepolia);

        let block = state.get_block(BlockNumber(750000)).unwrap();

        assert_eq!(block.transactions.len(), 10);
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
    }
}
