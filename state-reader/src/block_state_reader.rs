use crate::{error::StateReaderError, full_state_reader::FullStateReader};
use starknet_api::{
    block::BlockNumber,
    core::{ClassHash, CompiledClassHash, ContractAddress, Nonce},
    state::StorageKey,
};

use starknet_types_core::felt::Felt;

use blockifier::{
    execution::contract_class::RunnableCompiledClass,
    state::{
        errors::StateError,
        state_api::{StateReader, StateResult},
    },
};

/// A wrapper of [FullStateReader](`crate::full_state_reader::FullStateReader`)
/// for a particular block.
///
/// Used for as the state reader for executing transactions.
pub struct BlockStateReader<'s> {
    block_number: BlockNumber,
    reader: &'s FullStateReader,
}

impl<'s> BlockStateReader<'s> {
    pub fn new(block_number: BlockNumber, reader: &'s FullStateReader) -> Self {
        Self {
            block_number,
            reader,
        }
    }
}

impl StateReader for BlockStateReader<'_> {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        Ok(self
            .reader
            .get_storage_at(self.block_number, contract_address, key)?)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        Ok(self
            .reader
            .get_nonce_at(self.block_number, contract_address)?)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        Ok(self
            .reader
            .get_class_hash_at(self.block_number, contract_address)?)
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        Ok(self
            .reader
            .get_compiled_class(self.block_number, class_hash)?)
    }

    fn get_compiled_class_hash(&self, _: ClassHash) -> StateResult<CompiledClassHash> {
        todo!()
    }
}

impl From<StateReaderError> for StateError {
    fn from(value: StateReaderError) -> Self {
        StateError::StateReadError(value.to_string())
    }
}
