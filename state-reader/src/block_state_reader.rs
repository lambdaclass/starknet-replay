use crate::full_state_reader::FullStateReader;
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

pub struct BlockStateReader<'s> {
    block_number: BlockNumber,
    state_manager: &'s FullStateReader,
}

impl<'s> BlockStateReader<'s> {
    pub fn new(block_number: BlockNumber, state_manager: &'s FullStateReader) -> Self {
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
            .get_storage_at(self.block_number, contract_address, key)
            .map_err(to_state_error)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        self.state_manager
            .get_nonce_at(self.block_number, contract_address)
            .map_err(to_state_error)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        self.state_manager
            .get_class_hash_at(self.block_number, contract_address)
            .map_err(to_state_error)
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        self.state_manager
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
