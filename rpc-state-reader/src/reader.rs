use std::sync::Arc;

use blockifier::{
    execution::contract_class::{
        ContractClass, ContractClassV0, ContractClassV0Inner, NativeContractClassV1,
    },
    state::state_api::{StateReader, StateResult},
};
use cairo_vm::types::program::Program;
use starknet::core::types::ContractClass as SNContractClass;
use starknet_api::{
    block::BlockNumber,
    core::{ClassHash, CompiledClassHash, ContractAddress},
    state::StorageKey,
};
use starknet_gateway::{
    config::RpcStateReaderConfig, errors::serde_err_to_state_err,
    rpc_state_reader::RpcStateReader as GatewayRpcStateReader,
};
use ureq::json;

use crate::utils;

pub struct RpcStateReader {
    inner: GatewayRpcStateReader,
}

impl RpcStateReader {
    pub fn from_number(config: &RpcStateReaderConfig, block_number: BlockNumber) -> Self {
        Self {
            inner: GatewayRpcStateReader::from_number(config, block_number),
        }
    }

    pub fn get_contract_class(&self, class_hash: ClassHash) -> StateResult<SNContractClass> {
        let params = json!({
            "block_id": self.inner.block_id,
            "class_hash": class_hash.to_string(),
        });

        serde_json::from_value(
            self.inner
                .send_rpc_request("starknet_getClass", params.clone())?,
        )
        .map_err(serde_err_to_state_err)
    }
}

impl StateReader for RpcStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<cairo_vm::Felt252> {
        self.inner.get_storage_at(contract_address, key)
    }

    fn get_nonce_at(
        &self,
        contract_address: ContractAddress,
    ) -> StateResult<starknet_api::core::Nonce> {
        self.inner.get_nonce_at(contract_address)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        self.inner.get_class_hash_at(contract_address)
    }

    fn get_compiled_contract_class(&self, class_hash: ClassHash) -> StateResult<ContractClass> {
        Ok(match self.get_contract_class(class_hash)? {
            SNContractClass::Legacy(compressed_legacy_cc) => {
                let as_str = utils::decode_reader(compressed_legacy_cc.program).unwrap();
                let program = Program::from_bytes(as_str.as_bytes(), None).unwrap();
                let entry_points_by_type = utils::map_entry_points_by_type_legacy(
                    compressed_legacy_cc.entry_points_by_type,
                );
                let inner = Arc::new(ContractClassV0Inner {
                    program,
                    entry_points_by_type,
                });
                ContractClass::V0(ContractClassV0(inner))
            }
            SNContractClass::Sierra(flattened_sierra_cc) => {
                let middle_sierra: utils::MiddleSierraContractClass = {
                    let v = serde_json::to_value(flattened_sierra_cc).unwrap();
                    serde_json::from_value(v).unwrap()
                };
                let sierra_cc = cairo_lang_starknet_classes::contract_class::ContractClass {
                    sierra_program: middle_sierra.sierra_program,
                    contract_class_version: middle_sierra.contract_class_version,
                    entry_points_by_type: middle_sierra.entry_points_by_type,
                    sierra_program_debug_info: None,
                    abi: None,
                };

                if cfg!(feature = "only_casm") {
                    let casm_cc =
                    cairo_lang_starknet_classes::casm_contract_class::CasmContractClass::from_contract_class(sierra_cc, false, usize::MAX).unwrap();
                    ContractClass::V1(casm_cc.try_into().unwrap())
                } else {
                    let program = sierra_cc.extract_sierra_program().unwrap();
                    let executor = utils::get_native_executor(program, class_hash);

                    ContractClass::V1Native(
                        NativeContractClassV1::new(executor, sierra_cc).unwrap(),
                    )
                }
            }
        })
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.inner.get_compiled_class_hash(class_hash)
    }
}
