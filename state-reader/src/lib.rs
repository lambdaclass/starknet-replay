use std::io::{self, Read};

use cairo_vm::types::program::Program;
use remote_reader::RemoteReader;
use starknet_api::{
    block::BlockNumber,
    core::{ClassHash, CompiledClassHash, ContractAddress, Nonce},
    state::StorageKey,
};

use starknet_core::types::{CompressedLegacyContractClass, ContractClass, FlattenedSierraClass};
use starknet_types_core::felt::Felt;

use blockifier::{
    execution::contract_class::RunnableCompiledClass,
    state::{
        errors::StateError,
        state_api::{StateReader, StateResult},
    },
};

pub mod remote_reader;

pub struct StateManager {
    with_cairo_native: bool,
    block_number: BlockNumber,

    remote_reader: RemoteReader,
}

impl StateReader for StateManager {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        let result = match self.remote_reader.get_storage_at(contract_address, key) {
            Ok(x) => x,
            Err(err) => return Err(err.into()),
        };

        Ok(result)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        let result = match self.remote_reader.get_nonce_at(contract_address) {
            Ok(x) => x,
            Err(err) => return Err(err.into()),
        };

        Ok(result)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        let result = match self.remote_reader.get_class_hash_at(contract_address) {
            Ok(x) => x,
            Err(err) => return Err(err.into()),
        };

        Ok(result)
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        todo!();
    }

    fn get_compiled_class_hash(&self, _: ClassHash) -> StateResult<CompiledClassHash> {
        unimplemented!("compiled class hash is unused yet")
    }
}
