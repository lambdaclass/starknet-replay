use std::cell::RefCell;

use compiler::{
    compile_class, compile_v1_class, decompress_v0_class, processed_class_to_contract_class,
    ClassManagerError,
};
use objects::RpcTransactionReceipt;
use remote_reader::{RemoteReader, RemoteReaderError};
use starknet_api::{
    block::BlockNumber,
    contract_class::{ClassInfo, ContractClass as CompiledContractClass, SierraVersion},
    core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, Nonce},
    state::StorageKey,
    transaction::{Transaction, TransactionHash},
    StarknetApiError,
};

use starknet_core::types::{BlockWithTxHashes, ContractClass};
use starknet_types_core::felt::Felt;

use blockifier::{
    execution::contract_class::RunnableCompiledClass,
    state::{
        errors::StateError,
        state_api::{StateReader, StateResult},
    },
};
use state_cache::RemoteCache;
use thiserror::Error;

pub mod compiler;
pub mod objects;
pub mod remote_reader;
pub mod state_cache;

#[derive(Debug, Error)]
pub enum StateManagerError {
    #[error(transparent)]
    RemoteReaderError(#[from] RemoteReaderError),
    #[error(transparent)]
    ClassManagerError(#[from] ClassManagerError),
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    #[error("A legacy contract should always have an ABI")]
    LegacyContractWithoutAbi,
}

pub struct StateManager {
    remote_reader: RemoteReader,
    cache: RemoteCache,
}

impl StateManager {
    pub fn new(remote_reader: RemoteReader) -> Self {
        Self {
            remote_reader,
            cache: RemoteCache::load(),
        }
    }

    pub fn get_block(
        &mut self,
        block_number: BlockNumber,
    ) -> Result<BlockWithTxHashes, StateManagerError> {
        if let Some(result) = self.cache.blocks.get(&block_number) {
            return Ok(result.clone());
        }

        let result = self.remote_reader.get_block_with_tx_hashes(block_number)?;

        self.cache.blocks.insert(block_number, result.clone());

        Ok(result)
    }

    pub fn get_tx(&mut self, tx_hash: TransactionHash) -> Result<Transaction, StateManagerError> {
        if let Some(result) = self.cache.transactions.get(&tx_hash) {
            return Ok(result.clone());
        }

        let result = self.remote_reader.get_tx(&tx_hash)?;

        self.cache.transactions.insert(tx_hash, result.clone());

        Ok(result)
    }

    pub fn get_tx_receipt(
        &mut self,
        tx_hash: TransactionHash,
    ) -> Result<RpcTransactionReceipt, StateManagerError> {
        if let Some(result) = self.cache.transaction_receipts.get(&tx_hash) {
            return Ok(result.clone());
        }

        let result = self.remote_reader.get_tx_receipt(&tx_hash)?;

        self.cache
            .transaction_receipts
            .insert(tx_hash, result.clone());

        Ok(result)
    }

    pub fn get_storage_at(
        &mut self,
        block_number: BlockNumber,

        contract_address: ContractAddress,
        key: StorageKey,
    ) -> Result<Felt, StateManagerError> {
        if let Some(result) = self
            .cache
            .storage
            .get(&(block_number, contract_address, key))
        {
            return Ok(result.clone());
        }

        let result = self
            .remote_reader
            .get_storage_at(block_number, contract_address, key)?;

        self.cache
            .storage
            .insert((block_number, contract_address, key), result.clone());

        Ok(result)
    }

    pub fn get_nonce_at(
        &mut self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> Result<Nonce, StateManagerError> {
        if let Some(result) = self.cache.nonces.get(&(block_number, contract_address)) {
            return Ok(result.clone());
        }

        let result = self
            .remote_reader
            .get_nonce_at(block_number, contract_address)?;

        self.cache
            .nonces
            .insert((block_number, contract_address), result.clone());

        Ok(result)
    }

    pub fn get_class_hash_at(
        &mut self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> Result<ClassHash, StateManagerError> {
        if let Some(result) = self
            .cache
            .class_hashes
            .get(&(block_number, contract_address))
        {
            return Ok(result.clone());
        }

        let result = self
            .remote_reader
            .get_class_hash_at(block_number, contract_address)?;

        self.cache
            .class_hashes
            .insert((block_number, contract_address), result.clone());

        Ok(result)
    }

    pub fn get_contract_class(
        &mut self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<ContractClass, StateManagerError> {
        if let Some(result) = self.cache.contract_classes.get(&class_hash) {
            return Ok(result.clone());
        }

        let result = self
            .remote_reader
            .get_contract_class(block_number, &class_hash)?;

        self.cache
            .contract_classes
            .insert(class_hash, result.clone());

        Ok(result)
    }

    pub fn get_compiled_class(
        &mut self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<RunnableCompiledClass, StateManagerError> {
        let contract_class = self.get_contract_class(block_number, class_hash)?;

        let runnable_class = compile_class(&class_hash, contract_class)?;

        Ok(runnable_class)
    }

    pub fn get_class_info(
        &mut self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<ClassInfo, StateManagerError> {
        let contract_class = self.get_contract_class(block_number, class_hash)?;

        Ok(match contract_class {
            ContractClass::Legacy(legacy) => {
                let abi_length = legacy
                    .abi
                    .as_ref()
                    .ok_or(StateManagerError::LegacyContractWithoutAbi)?
                    .len();
                let compiled_class = decompress_v0_class(legacy)?;
                ClassInfo::new(
                    &CompiledContractClass::V0(compiled_class),
                    0,
                    abi_length,
                    SierraVersion::DEPRECATED,
                )?
            }
            ContractClass::Sierra(sierra) => {
                let abi_length = sierra.abi.len();
                let sierra_length = sierra.sierra_program.len();

                let class = processed_class_to_contract_class(sierra)?;
                let (compiled_class, sierra_version) = compile_v1_class(class)?;

                ClassInfo::new(
                    &CompiledContractClass::V1((compiled_class, sierra_version.clone())),
                    sierra_length,
                    abi_length,
                    sierra_version,
                )?
            }
        })
    }

    pub fn get_chain_id(&mut self) -> Result<ChainId, StateManagerError> {
        if let Some(result) = &self.cache.chain_id {
            return Ok(result.clone());
        }

        let chain_id = self.remote_reader.get_chain_id()?;

        let _ = self.cache.chain_id.insert(chain_id.clone());

        Ok(chain_id)
    }
}

impl Drop for StateManager {
    fn drop(&mut self) {
        self.cache.save()
    }
}

pub struct BlockStateReader<'s> {
    block_number: BlockNumber,
    state_manager: &'s RefCell<StateManager>,
}

impl<'s> BlockStateReader<'s> {
    pub fn new(block_number: BlockNumber, state_manager: &'s RefCell<StateManager>) -> Self {
        Self {
            block_number,
            state_manager,
        }
    }
}

impl<'s> StateReader for BlockStateReader<'s> {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        self.state_manager
            .borrow_mut()
            .get_storage_at(self.block_number, contract_address, key)
            .map_err(to_state_error)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        self.state_manager
            .borrow_mut()
            .get_nonce_at(self.block_number, contract_address)
            .map_err(to_state_error)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        self.state_manager
            .borrow_mut()
            .get_class_hash_at(self.block_number, contract_address)
            .map_err(to_state_error)
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        self.state_manager
            .borrow_mut()
            .get_compiled_class(self.block_number, class_hash)
            .map_err(to_state_error)
    }

    fn get_compiled_class_hash(&self, _: ClassHash) -> StateResult<CompiledClassHash> {
        todo!()
    }
}

pub fn to_state_error<E: std::error::Error>(error: E) -> StateError {
    StateError::StateReadError(error.to_string())
}

#[cfg(test)]
mod tests {
    use starknet_api::{
        block::BlockNumber, class_hash, contract_address, core::ChainId, felt, storage_key,
    };

    use crate::{
        remote_reader::{url_from_env, RemoteReader},
        StateManager,
    };

    #[test]
    pub fn get_contract_class() {
        let url = url_from_env(ChainId::Mainnet);
        let remote_reader = RemoteReader::new(url);

        let mut state = StateManager::new(remote_reader);

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
        let url = url_from_env(ChainId::Mainnet);
        let remote_reader = RemoteReader::new(url);

        let mut state = StateManager::new(remote_reader);

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
        let url = url_from_env(ChainId::Mainnet);
        let remote_reader = RemoteReader::new(url);

        let mut state = StateManager::new(remote_reader);

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
    }

    #[test]
    pub fn get_legacy_contract_class_info() {
        let url = url_from_env(ChainId::Mainnet);
        let remote_reader = RemoteReader::new(url);

        let mut state = StateManager::new(remote_reader);

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
    }

    #[test]
    pub fn get_cached_storage() {
        let url = url_from_env(ChainId::Mainnet);
        let remote_reader = RemoteReader::new(url);

        let mut state = StateManager::new(remote_reader);

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
        let url = url_from_env(ChainId::Mainnet);
        let remote_reader = RemoteReader::new(url);

        let mut state = StateManager::new(remote_reader);
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

        let url = url_from_env(ChainId::Mainnet);
        let remote_reader = RemoteReader::new(url);
        let mut state = StateManager::new(remote_reader);
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
}
