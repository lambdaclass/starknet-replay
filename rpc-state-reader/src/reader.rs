use std::{env, sync::Arc, thread, time::Duration};

use blockifier::{
    execution::{
        contract_class::{CompiledClassV0, CompiledClassV0Inner, RunnableCompiledClass},
        native::contract_class::NativeCompiledClassV1,
    },
    state::state_api::{StateReader as BlockifierStateReader, StateResult},
};
use cairo_lang_starknet_classes::contract_class::version_id_from_serialized_sierra_program;
use cairo_vm::types::program::Program;
use serde::Serialize;
use serde_json::Value;
use starknet::core::types::ContractClass as SNContractClass;
use starknet_api::{
    block::BlockNumber,
    core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, Nonce},
    state::StorageKey,
    transaction::{Transaction, TransactionHash},
};
use starknet_gateway::{
    config::RpcStateReaderConfig,
    errors::{serde_err_to_state_err, RPCStateReaderError, RPCStateReaderResult},
    rpc_objects::{
        GetBlockWithTxHashesParams, GetClassHashAtParams, GetNonceParams, GetStorageAtParams,
    },
    rpc_state_reader::RpcStateReader as GatewayRpcStateReader,
};
use tracing::{info_span, warn};
use ureq::json;

use crate::{
    objects::{self, BlockWithTxHahes, RpcTransactionReceipt, RpcTransactionTrace},
    utils::{self, bytecode_size, get_casm_compiled_class, get_native_executor},
};

const MAX_RETRIES: u32 = 10;
const RETRY_SLEEP_MS: u64 = 10000;

pub trait StateReader: BlockifierStateReader {
    fn get_block_with_tx_hashes(&self) -> StateResult<BlockWithTxHahes>;
    fn get_transaction(&self, hash: &TransactionHash) -> StateResult<Transaction>;
    fn get_contract_class(&self, class_hash: &ClassHash) -> StateResult<SNContractClass>;
    fn get_transaction_trace(&self, hash: &TransactionHash) -> StateResult<RpcTransactionTrace>;
    fn get_transaction_receipt(&self, hash: &TransactionHash)
        -> StateResult<RpcTransactionReceipt>;
    fn get_chain_id(&self) -> ChainId;
}

// The following structure is heavily inspired by the underlying starkware-libs/sequencer implementation.
// It uses sequencer's RpcStateReader under the hood in some situations, while in other situation
// the actual implementation has been copied and modified to our needs.

pub struct RpcStateReader {
    chain: ChainId,
    pub block_number: BlockNumber,
    inner: GatewayRpcStateReader,
}

impl RpcStateReader {
    pub fn new(chain: ChainId, block_number: BlockNumber) -> Self {
        let config = build_config(&chain);

        Self {
            inner: GatewayRpcStateReader::from_number(&config, block_number),
            chain,
            block_number,
        }
    }

    pub fn send_rpc_request_with_retry(
        &self,
        method: &str,
        params: impl Serialize,
    ) -> RPCStateReaderResult<Value> {
        let result = retry(|| self.inner.send_rpc_request(method, &params));

        if let Err(RPCStateReaderError::ReqwestError(err)) = result {
            Err(RPCStateReaderError::ReqwestError(err.without_url()))
        } else {
            result
        }
    }
}

impl StateReader for RpcStateReader {
    fn get_contract_class(&self, class_hash: &ClassHash) -> StateResult<SNContractClass> {
        let params = json!({
            "block_id": self.inner.block_id,
            "class_hash": class_hash.to_hex_string(),
        });

        serde_json::from_value(self.send_rpc_request_with_retry("starknet_getClass", params)?)
            .map_err(serde_err_to_state_err)
    }

    fn get_transaction_trace(&self, hash: &TransactionHash) -> StateResult<RpcTransactionTrace> {
        let params = json!([hash]);

        serde_json::from_value(
            self.send_rpc_request_with_retry("starknet_traceTransaction", params)?,
        )
        .map_err(serde_err_to_state_err)
    }

    fn get_transaction(&self, hash: &TransactionHash) -> StateResult<Transaction> {
        let params = json!([hash]);

        let tx = self.send_rpc_request_with_retry("starknet_getTransactionByHash", params)?;

        objects::deser::transaction_from_json(tx).map_err(serde_err_to_state_err)
    }

    fn get_block_with_tx_hashes(&self) -> StateResult<BlockWithTxHahes> {
        let params = GetBlockWithTxHashesParams {
            block_id: self.inner.block_id,
        };

        serde_json::from_value(
            self.send_rpc_request_with_retry("starknet_getBlockWithTxHashes", params)?,
        )
        .map_err(serde_err_to_state_err)
    }

    fn get_transaction_receipt(
        &self,
        hash: &TransactionHash,
    ) -> StateResult<RpcTransactionReceipt> {
        let params = json!([hash]);

        serde_json::from_value(
            self.send_rpc_request_with_retry("starknet_getTransactionReceipt", params)?,
        )
        .map_err(serde_err_to_state_err)
    }

    fn get_chain_id(&self) -> ChainId {
        self.chain.clone()
    }
}

fn build_config(chain: &ChainId) -> RpcStateReaderConfig {
    let url = match chain {
        ChainId::Mainnet => {
            env::var("RPC_ENDPOINT_MAINNET").expect("Missing env var: RPC_ENDPOINT_MAINNET")
        }
        ChainId::Sepolia => {
            env::var("RPC_ENDPOINT_TESTNET").expect("Missing env var: RPC_ENDPOINT_TESTNET")
        }
        ChainId::IntegrationSepolia => todo!(),
        ChainId::Other(_) => todo!(),
    };

    RpcStateReaderConfig {
        url,
        json_rpc_version: "2.0".to_string(),
    }
}

impl BlockifierStateReader for RpcStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<cairo_vm::Felt252> {
        let get_storage_at_params = GetStorageAtParams {
            block_id: self.inner.block_id,
            contract_address,
            key,
        };

        let result =
            self.send_rpc_request_with_retry("starknet_getStorageAt", &get_storage_at_params);
        match result {
            Ok(value) => Ok(serde_json::from_value(value).map_err(serde_err_to_state_err)?),
            Err(RPCStateReaderError::ContractAddressNotFound(_)) => {
                Ok(cairo_vm::Felt252::default())
            }
            Err(e) => Err(e)?,
        }
    }

    fn get_nonce_at(
        &self,
        contract_address: ContractAddress,
    ) -> StateResult<starknet_api::core::Nonce> {
        let get_nonce_params = GetNonceParams {
            block_id: self.inner.block_id,
            contract_address,
        };

        let result = self.send_rpc_request_with_retry("starknet_getNonce", get_nonce_params);
        match result {
            Ok(value) => {
                let nonce: Nonce = serde_json::from_value(value).map_err(serde_err_to_state_err)?;
                Ok(nonce)
            }
            Err(RPCStateReaderError::ContractAddressNotFound(_)) => Ok(Nonce::default()),
            Err(e) => Err(e)?,
        }
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        let get_class_hash_at_params = GetClassHashAtParams {
            contract_address,
            block_id: self.inner.block_id,
        };

        let result =
            self.send_rpc_request_with_retry("starknet_getClassHashAt", get_class_hash_at_params);
        match result {
            Ok(value) => {
                let class_hash: ClassHash =
                    serde_json::from_value(value).map_err(serde_err_to_state_err)?;
                Ok(class_hash)
            }
            Err(RPCStateReaderError::ContractAddressNotFound(_)) => Ok(ClassHash::default()),
            Err(e) => Err(e)?,
        }
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        Ok(compile_contract_class(
            self.get_contract_class(&class_hash)?,
            class_hash,
        ))
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.inner.get_compiled_class_hash(class_hash)
    }
}

pub fn compile_contract_class(class: SNContractClass, hash: ClassHash) -> RunnableCompiledClass {
    match class {
        SNContractClass::Legacy(compressed_legacy_cc) => compile_legacy_cc(compressed_legacy_cc),
        SNContractClass::Sierra(flattened_sierra_cc) => {
            compile_sierra_cc(flattened_sierra_cc, hash)
        }
    }
}

fn compile_sierra_cc(
    flattened_sierra_cc: starknet::core::types::FlattenedSierraClass,
    class_hash: ClassHash,
) -> RunnableCompiledClass {
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

    let _span = info_span!(
        "contract compilation",
        class_hash = class_hash.to_hex_string(),
        length = bytecode_size(&sierra_cc.sierra_program)
    )
    .entered();

    if cfg!(feature = "only_casm") {
        let casm_compiled_class = get_casm_compiled_class(sierra_cc, class_hash);
        RunnableCompiledClass::V1(casm_compiled_class)
    } else {
        let executor = if cfg!(feature = "with-sierra-emu") {
            let (sierra_version, _) =
                version_id_from_serialized_sierra_program(&sierra_cc.sierra_program).unwrap();
            let program = Arc::new(sierra_cc.extract_sierra_program().unwrap());
            (
                program,
                sierra_cc.entry_points_by_type.clone(),
                sierra_version,
            )
                .into()
        } else {
            #[cfg(feature = "with-trace-dump")]
            {
                ContractExecutor::AotTrace((
                    get_native_executor(&sierra_cc, class_hash),
                    sierra_cc.extract_sierra_program().unwrap(),
                ))
            }
            #[cfg(not(feature = "with-trace-dump"))]
            {
                get_native_executor(&sierra_cc, class_hash).into()
            }
        };

        let casm_compiled_class = get_casm_compiled_class(sierra_cc, class_hash);

        RunnableCompiledClass::V1Native(NativeCompiledClassV1::new(executor, casm_compiled_class))
    }
}

fn compile_legacy_cc(
    compressed_legacy_cc: starknet::core::types::CompressedLegacyContractClass,
) -> RunnableCompiledClass {
    let as_str = utils::decode_reader(compressed_legacy_cc.program).unwrap();
    let program = Program::from_bytes(as_str.as_bytes(), None).unwrap();
    let entry_points_by_type =
        utils::map_entry_points_by_type_legacy(compressed_legacy_cc.entry_points_by_type);
    let inner = Arc::new(CompiledClassV0Inner {
        program,
        entry_points_by_type,
    });
    warn!("Using Cairo 0 contract class: this transaction will no longer execute with native");
    RunnableCompiledClass::V0(CompiledClassV0(inner))
}

/// Retries the closure `MAX_RETRIES` times on RPC errors,
/// waiting RETRY_SLEEP_MS after each retry
fn retry(f: impl Fn() -> RPCStateReaderResult<Value>) -> RPCStateReaderResult<Value> {
    let mut attempt = 0;
    loop {
        let result = f();
        attempt += 1;

        // only retry on rpc or request error
        if !matches!(
            result,
            Err(RPCStateReaderError::RPCError(_) | RPCStateReaderError::ReqwestError(_))
        ) {
            return result;
        }

        if attempt >= MAX_RETRIES {
            return result;
        }

        thread::sleep(Duration::from_millis(RETRY_SLEEP_MS))
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_get_block_with_tx_hashes() {
        let reader = RpcStateReader::new(ChainId::Mainnet, BlockNumber(397709));

        let block = reader.get_block_with_tx_hashes().unwrap();

        assert_eq!(block.transactions.len(), 211);
    }
}
