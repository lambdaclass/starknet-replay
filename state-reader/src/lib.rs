use compiler::compile_class;
use remote_reader::RemoteReader;
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

pub mod compiler;
pub mod remote_reader;

pub struct StateManager {
    block_number: BlockNumber,
    remote_reader: RemoteReader,
}

impl StateManager {
    pub fn new(block_number: BlockNumber, remote_reader: RemoteReader) -> Self {
        Self {
            block_number,
            remote_reader,
        }
    }
}

impl StateReader for StateManager {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        let result =
            match self
                .remote_reader
                .get_storage_at(self.block_number, contract_address, key)
            {
                Ok(x) => x,
                Err(err) => return Err(err.into()),
            };

        Ok(result)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        let result = match self
            .remote_reader
            .get_nonce_at(self.block_number, contract_address)
        {
            Ok(x) => x,
            Err(err) => return Err(err.into()),
        };

        Ok(result)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        let result = match self
            .remote_reader
            .get_class_hash_at(self.block_number, contract_address)
        {
            Ok(x) => x,
            Err(err) => return Err(err.into()),
        };

        Ok(result)
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        let contract_class = match self
            .remote_reader
            .get_contract_class(self.block_number, &class_hash)
        {
            Ok(x) => x,
            Err(err) => return Err(err.into()),
        };

        let runnable_class = compile_class(&class_hash, contract_class)
            .map_err(|err| StateError::StateReadError(err.to_string()))?;

        Ok(runnable_class)
    }

    fn get_compiled_class_hash(&self, _: ClassHash) -> StateResult<CompiledClassHash> {
        unimplemented!("compiled class hash is unused yet")
    }
}

#[cfg(test)]
mod tests {
    use blockifier::state::state_api::StateReader;
    use starknet_api::{block::BlockNumber, class_hash, core::ChainId};

    use crate::{
        remote_reader::{url_from_env, RemoteReader},
        StateManager,
    };

    #[test]
    pub fn get_contract_class() {
        let url = url_from_env(ChainId::Mainnet);
        let remote_reader = RemoteReader::new(url);

        let state = StateManager::new(BlockNumber(1500000), remote_reader);

        state
            .get_compiled_class(class_hash!(
                "0x07f3331378862ed0a10f8c3d49f4650eb845af48f1c8120591a43da8f6f12679"
            ))
            .unwrap();

        state
            .get_compiled_class(class_hash!(
                "0x07f3331378862ed0a10f8c3d49f4650eb845af48f1c8120591a43da8f6f12679"
            ))
            .unwrap();
    }

    #[test]
    pub fn get_legacy_contract_class() {
        let url = url_from_env(ChainId::Mainnet);
        let remote_reader = RemoteReader::new(url);

        let state = StateManager::new(BlockNumber(1500000), remote_reader);

        state
            .get_compiled_class(class_hash!(
                "0x010455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8"
            ))
            .unwrap();

        state
            .get_compiled_class(class_hash!(
                "0x010455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8"
            ))
            .unwrap();
    }
}
