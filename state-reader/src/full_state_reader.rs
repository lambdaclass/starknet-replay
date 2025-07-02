use std::cell::RefCell;

use crate::class_manager::{ClassManager, ClassManagerError};
use crate::objects::RpcTransactionReceipt;
use crate::remote_state_reader::{RemoteStateReader, RemoteStateReaderError};
use blockifier::execution::native::contract_class::NativeCompiledClassV1;
use blockifier::execution::native::executor::ContractExecutor;
use cairo_vm::types::errors::program_errors::ProgramError;
use starknet_api::{
    block::BlockNumber,
    contract_class::{ClassInfo, ContractClass as CompiledContractClass, SierraVersion},
    core::{ChainId, ClassHash, ContractAddress, Nonce},
    state::StorageKey,
    transaction::{Transaction, TransactionHash},
    StarknetApiError,
};

use starknet_core::types::{BlockWithTxHashes, ContractClass};
use starknet_types_core::felt::Felt;

use crate::remote_state_cache::RemoteStateCache;
use blockifier::execution::contract_class::{
    CompiledClassV0, CompiledClassV1, RunnableCompiledClass,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FullStateReaderError {
    #[error(transparent)]
    RemoteReaderError(#[from] RemoteStateReaderError),
    #[error(transparent)]
    ClassManagerError(#[from] ClassManagerError),
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    #[error(transparent)]
    ProgramError(#[from] ProgramError),
    #[error("a legacy contract should always have an ABI")]
    LegacyContractWithoutAbi,
    #[error("could not find requested value")]
    NotFound,
}

pub struct FullStateReader {
    pub with_remote: bool,
    pub with_compilation: bool,
    remote_reader: RemoteStateReader,
    remote_cache: RefCell<RemoteStateCache>,
    class_manager: RefCell<ClassManager>,
}

impl FullStateReader {
    pub fn new(remote_reader: RemoteStateReader) -> Self {
        Self {
            with_remote: true,
            with_compilation: true,
            remote_reader,
            remote_cache: RefCell::new(RemoteStateCache::load()),
            class_manager: RefCell::new(ClassManager::new()),
        }
    }

    pub fn get_block(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockWithTxHashes, FullStateReaderError> {
        if let Some(result) = self.remote_cache.borrow().blocks.get(&block_number) {
            return Ok(result.clone());
        }

        if self.with_remote {
            let result = self.remote_reader.get_block_with_tx_hashes(block_number)?;

            self.remote_cache
                .borrow_mut()
                .blocks
                .insert(block_number, result.clone());

            return Ok(result);
        }

        Err(FullStateReaderError::NotFound)
    }

    pub fn get_tx(&self, tx_hash: TransactionHash) -> Result<Transaction, FullStateReaderError> {
        if let Some(result) = self.remote_cache.borrow().transactions.get(&tx_hash) {
            return Ok(result.clone());
        }

        if self.with_remote {
            let result = self.remote_reader.get_tx(&tx_hash)?;

            self.remote_cache
                .borrow_mut()
                .transactions
                .insert(tx_hash, result.clone());

            return Ok(result);
        }

        Err(FullStateReaderError::NotFound)
    }

    pub fn get_tx_receipt(
        &self,
        tx_hash: TransactionHash,
    ) -> Result<RpcTransactionReceipt, FullStateReaderError> {
        if let Some(result) = self
            .remote_cache
            .borrow()
            .transaction_receipts
            .get(&tx_hash)
        {
            return Ok(result.clone());
        }

        if self.with_remote {
            let result = self.remote_reader.get_tx_receipt(&tx_hash)?;
            self.remote_cache
                .borrow_mut()
                .transaction_receipts
                .insert(tx_hash, result.clone());

            return Ok(result);
        }

        Err(FullStateReaderError::NotFound)
    }

    pub fn get_storage_at(
        &self,
        block_number: BlockNumber,

        contract_address: ContractAddress,
        key: StorageKey,
    ) -> Result<Felt, FullStateReaderError> {
        if let Some(result) =
            self.remote_cache
                .borrow()
                .storage
                .get(&(block_number, contract_address, key))
        {
            return Ok(*result);
        }

        if self.with_remote {
            let result = self
                .remote_reader
                .get_storage_at(block_number, contract_address, key)?;

            self.remote_cache
                .borrow_mut()
                .storage
                .insert((block_number, contract_address, key), result);
            return Ok(result);
        }

        Err(FullStateReaderError::NotFound)
    }

    pub fn get_nonce_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> Result<Nonce, FullStateReaderError> {
        if let Some(result) = self
            .remote_cache
            .borrow()
            .nonces
            .get(&(block_number, contract_address))
        {
            return Ok(*result);
        }

        if self.with_remote {
            let result = self
                .remote_reader
                .get_nonce_at(block_number, contract_address)?;

            self.remote_cache
                .borrow_mut()
                .nonces
                .insert((block_number, contract_address), result);

            return Ok(result);
        }

        Err(FullStateReaderError::NotFound)
    }

    pub fn get_class_hash_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> Result<ClassHash, FullStateReaderError> {
        if let Some(result) = self
            .remote_cache
            .borrow()
            .class_hashes
            .get(&(block_number, contract_address))
        {
            return Ok(*result);
        }

        if self.with_remote {
            let result = self
                .remote_reader
                .get_class_hash_at(block_number, contract_address)?;

            self.remote_cache
                .borrow_mut()
                .class_hashes
                .insert((block_number, contract_address), result);

            return Ok(result);
        }

        Err(FullStateReaderError::NotFound)
    }

    pub fn get_contract_class(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<ContractClass, FullStateReaderError> {
        if let Some(result) = self.remote_cache.borrow().contract_classes.get(&class_hash) {
            return Ok(result.clone());
        }

        if self.with_remote {
            let result = self
                .remote_reader
                .get_contract_class(block_number, &class_hash)?;

            self.remote_cache
                .borrow_mut()
                .contract_classes
                .insert(class_hash, result.clone());

            return Ok(result);
        }

        Err(FullStateReaderError::NotFound)
    }

    pub fn get_compiled_class(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<RunnableCompiledClass, FullStateReaderError> {
        let mut class_manager = self.class_manager.borrow_mut();
        let casm_class = match class_manager.get_casm_class(&class_hash) {
            Some(casm_class) => casm_class,
            None => {
                if !self.with_compilation {
                    return Err(FullStateReaderError::NotFound);
                }

                let contract_class = self.get_contract_class(block_number, class_hash)?;
                class_manager.compile_casm_class(&class_hash, contract_class)?
            }
        };

        let native_executor = if cfg!(feature = "only-casm") {
            None
        } else {
            match class_manager.get_native_executor(&class_hash) {
                Some(native_executor) => Some(native_executor),
                None => {
                    let contract_class = self.get_contract_class(block_number, class_hash)?;
                    if let ContractClass::Sierra(sierra_class) = contract_class {
                        if !self.with_compilation {
                            return Err(FullStateReaderError::NotFound);
                        }

                        Some(class_manager.compile_native_class(&class_hash, sierra_class)?)
                    } else {
                        None
                    }
                }
            }
        };

        Ok(match casm_class {
            CompiledContractClass::V0(deprecated_class) => {
                RunnableCompiledClass::V0(CompiledClassV0::try_from(deprecated_class)?)
            }
            CompiledContractClass::V1(versioned_casm) => {
                let casm_class = CompiledClassV1::try_from(versioned_casm)?;

                match native_executor {
                    Some(native_executor) => {
                        let contract_executor = ContractExecutor::Aot(native_executor);
                        RunnableCompiledClass::V1Native(NativeCompiledClassV1::new(
                            contract_executor,
                            casm_class,
                        ))
                    }
                    None => RunnableCompiledClass::V1(casm_class),
                }
            }
        })
    }

    pub fn get_class_info(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<ClassInfo, FullStateReaderError> {
        let contract_class = self.get_contract_class(block_number, class_hash)?;

        let mut class_manager = self.class_manager.borrow_mut();
        let casm_class = match class_manager.get_casm_class(&class_hash) {
            Some(casm_class) => casm_class,
            None => {
                if !self.with_compilation {
                    return Err(FullStateReaderError::NotFound);
                }
                class_manager.compile_casm_class(&class_hash, contract_class.clone())?
            }
        };

        Ok(match contract_class {
            ContractClass::Legacy(legacy) => {
                let abi_length = legacy
                    .abi
                    .as_ref()
                    .ok_or(FullStateReaderError::LegacyContractWithoutAbi)?
                    .len();
                ClassInfo::new(&casm_class, 0, abi_length, SierraVersion::DEPRECATED)?
            }
            ContractClass::Sierra(sierra) => {
                let abi_length = sierra.abi.len();
                let sierra_length = sierra.sierra_program.len();

                let sierra_version = match &casm_class {
                    CompiledContractClass::V0(_) => {
                        panic!("a sierra class cannot have a deprecated compiled class")
                    }
                    CompiledContractClass::V1((_, version)) => version.clone(),
                };
                ClassInfo::new(&casm_class, sierra_length, abi_length, sierra_version)?
            }
        })
    }

    pub fn get_chain_id(&self) -> Result<ChainId, FullStateReaderError> {
        if let Some(result) = &self.remote_cache.borrow().chain_id {
            return Ok(result.clone());
        }

        let chain_id = self.remote_reader.get_chain_id()?;

        let _ = self
            .remote_cache
            .borrow_mut()
            .chain_id
            .insert(chain_id.clone());

        Ok(chain_id)
    }
}

impl Drop for FullStateReader {
    fn drop(&mut self) {
        self.remote_cache.borrow_mut().save()
    }
}

#[cfg(test)]
mod tests {
    use starknet_api::{
        block::BlockNumber, class_hash, contract_address, core::ChainId, felt, storage_key,
    };

    use crate::{
        full_state_reader::FullStateReader,
        remote_state_reader::{url_from_env, RemoteStateReader},
    };

    #[test]
    pub fn get_contract_class() {
        let url = url_from_env(ChainId::Mainnet);
        let remote_reader = RemoteStateReader::new(url);

        let mut state = FullStateReader::new(remote_reader);

        state
            .get_compiled_class(
                BlockNumber(1500000),
                class_hash!("0x07f3331378862ed0a10f8c3d49f4650eb845af48f1c8120591a43da8f6f12679"),
            )
            .unwrap();

        state.with_compilation = false;
        state.with_remote = false;

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
        let remote_reader = RemoteStateReader::new(url);

        let mut state = FullStateReader::new(remote_reader);

        state
            .get_compiled_class(
                BlockNumber(1500000),
                class_hash!("0x010455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8"),
            )
            .unwrap();

        state.with_compilation = false;
        state.with_remote = false;

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
        let remote_reader = RemoteStateReader::new(url);

        let mut state = FullStateReader::new(remote_reader);

        state
            .get_class_info(
                BlockNumber(1500000),
                class_hash!("0x07f3331378862ed0a10f8c3d49f4650eb845af48f1c8120591a43da8f6f12679"),
            )
            .unwrap();

        state.with_compilation = false;
        state.with_remote = false;

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
        let remote_reader = RemoteStateReader::new(url);

        let mut state = FullStateReader::new(remote_reader);

        state
            .get_class_info(
                BlockNumber(1500000),
                class_hash!("0x010455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8"),
            )
            .unwrap();

        state.with_compilation = false;
        state.with_remote = false;

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
        let remote_reader = RemoteStateReader::new(url);

        let mut state = FullStateReader::new(remote_reader);

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

        state.with_remote = false;

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
        let remote_reader = RemoteStateReader::new(url);

        let state = FullStateReader::new(remote_reader);

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
        let remote_reader = RemoteStateReader::new(url);

        let mut state = FullStateReader::new(remote_reader);
        state.with_remote = false;

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
