use std::cell::RefCell;

use crate::compiler::{
    compile_class, compile_v1_class, decompress_v0_class, processed_class_to_contract_class,
    ClassManagerError,
};
use crate::objects::RpcTransactionReceipt;
use crate::remote_state_reader::{RemoteStateReader, RemoteStateReaderError};
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
use blockifier::execution::contract_class::RunnableCompiledClass;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FullStateReaderError {
    #[error(transparent)]
    RemoteReaderError(#[from] RemoteStateReaderError),
    #[error(transparent)]
    ClassManagerError(#[from] ClassManagerError),
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    #[error("A legacy contract should always have an ABI")]
    LegacyContractWithoutAbi,
}

pub struct FullStateReader {
    remote_reader: RemoteStateReader,
    cache: RefCell<RemoteStateCache>,
}

impl FullStateReader {
    pub fn new(remote_reader: RemoteStateReader) -> Self {
        Self {
            remote_reader,
            cache: RefCell::new(RemoteStateCache::load()),
        }
    }

    pub fn get_block(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockWithTxHashes, FullStateReaderError> {
        if let Some(result) = self.cache.borrow().blocks.get(&block_number) {
            return Ok(result.clone());
        }

        let result = self.remote_reader.get_block_with_tx_hashes(block_number)?;

        self.cache
            .borrow_mut()
            .blocks
            .insert(block_number, result.clone());

        Ok(result)
    }

    pub fn get_tx(&self, tx_hash: TransactionHash) -> Result<Transaction, FullStateReaderError> {
        if let Some(result) = self.cache.borrow().transactions.get(&tx_hash) {
            return Ok(result.clone());
        }

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
            return Ok(result.clone());
        }

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
            return Ok(*result);
        }

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
            return Ok(*result);
        }

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
            return Ok(*result);
        }

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
            return Ok(result.clone());
        }

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
        let contract_class = self.get_contract_class(block_number, class_hash)?;

        let runnable_class = compile_class(&class_hash, contract_class)?;

        Ok(runnable_class)
    }

    pub fn get_class_info(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> Result<ClassInfo, FullStateReaderError> {
        let contract_class = self.get_contract_class(block_number, class_hash)?;

        Ok(match contract_class {
            ContractClass::Legacy(legacy) => {
                let abi_length = legacy
                    .abi
                    .as_ref()
                    .ok_or(FullStateReaderError::LegacyContractWithoutAbi)?
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

    pub fn get_chain_id(&self) -> Result<ChainId, FullStateReaderError> {
        if let Some(result) = &self.cache.borrow().chain_id {
            return Ok(result.clone());
        }

        let chain_id = self.remote_reader.get_chain_id()?;

        let _ = self.cache.borrow_mut().chain_id.insert(chain_id.clone());

        Ok(chain_id)
    }
}

impl Drop for FullStateReader {
    fn drop(&mut self) {
        self.cache.borrow_mut().save()
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

        let state = FullStateReader::new(remote_reader);

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
        let remote_reader = RemoteStateReader::new(url);

        let state = FullStateReader::new(remote_reader);

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
        let remote_reader = RemoteStateReader::new(url);

        let state = FullStateReader::new(remote_reader);

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
        let remote_reader = RemoteStateReader::new(url);

        let state = FullStateReader::new(remote_reader);

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
    }
}
