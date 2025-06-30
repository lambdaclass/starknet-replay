use remote_reader::RemoteReader;
use starknet_api::{
    block::BlockNumber,
    core::{ClassHash, CompiledClassHash, ContractAddress, Nonce},
    state::StorageKey,
};

use starknet_types_core::felt::Felt;

use blockifier::{
    execution::contract_class::RunnableCompiledClass,
    state::state_api::{StateReader, StateResult},
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
        todo!()
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        todo!()
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        todo!()
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        todo!()
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        todo!()
    }
}
