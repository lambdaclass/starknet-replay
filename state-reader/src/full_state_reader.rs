use std::cell::{Cell, RefCell};

use crate::class_manager::ClassManager;
use crate::error::StateReaderError;
use crate::objects::RpcTransactionReceipt;
use crate::remote_state_reader::{url_from_env, RemoteStateReader};
use starknet_api::{
    block::BlockNumber,
    contract_class::ClassInfo,
    core::{ChainId, ClassHash, ContractAddress, Nonce},
    state::StorageKey,
    transaction::{Transaction, TransactionHash},
};

use starknet_core::types::{BlockWithTxHashes, ContractClass};
use starknet_types_core::felt::Felt;

use crate::disk_state_reader::DiskStateReader;
use blockifier::execution::contract_class::RunnableCompiledClass;

/// Reader and cache for a Starknet node's state.
///
/// Dropping this reader may take a while, as it has to save data to disk.
///
/// As the blockifier state reader expectes an inmutable reference, we need a
/// `RefCell`/`Cell` allow method signatures to receive inmutable references. As there
/// is no recursion, a runtime panic is impossible.
pub struct FullStateReader {
    chain_id: ChainId,
    hit_counter: Cell<u64>,
    miss_counter: Cell<u64>,
    disk_reader: RefCell<DiskStateReader>,
    remote_reader: RemoteStateReader,
    class_manager: RefCell<ClassManager>,
}

impl FullStateReader {
    pub fn new(chain_id: ChainId) -> Self {
        let remote_url = url_from_env(&chain_id);
        let remote_reader = RemoteStateReader::new(remote_url);
        Self {
            remote_reader,
            disk_reader: RefCell::new(DiskStateReader::new(chain_id.clone())),
            class_manager: RefCell::new(ClassManager::new()),
            hit_counter: Cell::new(0),
            miss_counter: Cell::new(0),
            chain_id,
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
    ) -> Result<BlockWithTxHashes, StateReaderError> {
        if let Ok(block) = self.disk_reader.borrow_mut().get_block(block_number) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(block.clone());
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        let block = self.remote_reader.get_block_with_tx_hashes(block_number)?;

        self.disk_reader
            .borrow_mut()
            .set_block(block_number, block.clone())?;

        Ok(block)
    }

    pub fn get_tx(&self, tx_hash: TransactionHash) -> Result<Transaction, StateReaderError> {
        if let Ok(tx) = self.disk_reader.borrow_mut().get_transaction(tx_hash) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(tx.clone());
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        let tx = self.remote_reader.get_tx(&tx_hash)?;

        self.disk_reader
            .borrow_mut()
            .set_transaction(tx_hash, tx.clone())?;

        Ok(tx)
    }

    pub fn get_tx_receipt(
        &self,
        tx_hash: TransactionHash,
    ) -> Result<RpcTransactionReceipt, StateReaderError> {
        if let Ok(receipt) = self
            .disk_reader
            .borrow_mut()
            .get_transaction_receipt(tx_hash)
        {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(receipt.clone());
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        let receipt = self.remote_reader.get_tx_receipt(&tx_hash)?;
        self.disk_reader
            .borrow_mut()
            .set_transaction_receipt(tx_hash, receipt.clone())?;

        Ok(receipt)
    }

    pub fn get_storage_at(
        &self,
        block_number: BlockNumber,

        contract_address: ContractAddress,
        key: StorageKey,
    ) -> Result<Felt, StateReaderError> {
        let mut cache = self.disk_reader.borrow_mut();
        let block_cache = cache.get_block_state_cache(block_number)?;

        if let Some(&value) = &block_cache.storage.get(&(contract_address, key)) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(value);
        }

        self.miss_counter.set(self.miss_counter.get() + 1);
        let value = self
            .remote_reader
            .get_storage_at(block_number, contract_address, key)?;
        block_cache.storage.insert((contract_address, key), value);

        Ok(value)
    }

    pub fn get_nonce_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> Result<Nonce, StateReaderError> {
        let mut cache = self.disk_reader.borrow_mut();
        let block_cache = cache.get_block_state_cache(block_number)?;

        if let Some(&nonce) = &block_cache.nonces.get(&contract_address) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(nonce);
        }

        self.miss_counter.set(self.miss_counter.get() + 1);
        let nonce = self
            .remote_reader
            .get_nonce_at(block_number, contract_address)?;
        block_cache.nonces.insert(contract_address, nonce);

        Ok(nonce)
    }

    pub fn get_class_hash_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> Result<ClassHash, StateReaderError> {
        let mut cache = self.disk_reader.borrow_mut();
        let block_cache = cache.get_block_state_cache(block_number)?;

        if let Some(&class_hash) = &block_cache.class_hashes.get(&contract_address) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(class_hash);
        }

        self.miss_counter.set(self.miss_counter.get() + 1);
        let class_hash = self
            .remote_reader
            .get_class_hash_at(block_number, contract_address)?;
        block_cache
            .class_hashes
            .insert(contract_address, class_hash);

        Ok(class_hash)
    }

    pub fn get_contract_class(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<ContractClass, StateReaderError> {
        if let Ok(receipt) = self.disk_reader.borrow_mut().get_contract_class(class_hash) {
            self.hit_counter.set(self.hit_counter.get() + 1);
            return Ok(receipt.clone());
        }
        self.miss_counter.set(self.miss_counter.get() + 1);

        let contract_class = self
            .remote_reader
            .get_contract_class(block_number, &class_hash)?;
        self.disk_reader
            .borrow_mut()
            .set_contract_class(class_hash, contract_class.clone())?;

        Ok(contract_class)
    }

    pub fn get_compiled_class(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<RunnableCompiledClass, StateReaderError> {
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
    ) -> Result<ClassInfo, StateReaderError> {
        let contract_class = self.get_contract_class(block_number, class_hash)?;

        // This value is not cached in memory as its only used before executing
        // a transaction. This means that it won't affect a transaction
        // performance.
        Ok(self
            .class_manager
            .borrow()
            .get_class_info(&class_hash, contract_class)?)
    }

    pub fn get_chain_id(&self) -> Result<ChainId, StateReaderError> {
        Ok(self.chain_id.clone())
    }
}

impl Drop for FullStateReader {
    fn drop(&mut self) {
        let _ = self.disk_reader.borrow_mut().save();
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
