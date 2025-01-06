use std::{
    env, fmt,
    fs::File,
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use blockifier::{
    blockifier::block::validated_gas_prices,
    execution::{
        contract_class::{
            CompiledClassV0, CompiledClassV0Inner, CompiledClassV1, RunnableCompiledClass,
        },
        native::contract_class::NativeCompiledClassV1,
    },
    state::state_api::{StateReader, StateResult},
};
use blockifier_reexecution::state_reader::compile::{
    legacy_to_contract_class_v0, sierra_to_contact_class_v1,
};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_utils::bigint::BigUintAsHex;
use cairo_vm::types::program::Program;
use serde::Serialize;
use serde_json::Value;
use starknet::core::types::ContractClass as SNContractClass;
use starknet_api::{
    block::{BlockInfo, BlockNumber, GasPrice, NonzeroGasPrice},
    contract_class::{ClassInfo, SierraVersion},
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
use tracing::{info, info_span};
use ureq::json;

use crate::{
    cache::RpcCachedState,
    objects::{
        self, BlockHeader, BlockWithTxHahes, BlockWithTxs, RpcTransactionReceipt,
        RpcTransactionTrace,
    },
    utils,
};

/// Starknet chains supported in Infura.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum RpcChain {
    MainNet,
    TestNet,
    TestNet2,
}

impl fmt::Display for RpcChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RpcChain::MainNet => write!(f, "starknet-mainnet"),
            RpcChain::TestNet => write!(f, "starknet-goerli"),
            RpcChain::TestNet2 => write!(f, "starknet-goerli2"),
        }
    }
}

impl From<RpcChain> for ChainId {
    fn from(value: RpcChain) -> Self {
        ChainId::Other(match value {
            RpcChain::MainNet => "alpha-mainnet".to_string(),
            RpcChain::TestNet => "alpha4".to_string(),
            RpcChain::TestNet2 => "alpha4-2".to_string(),
        })
    }
}

const MAX_RETRIES: u32 = 10;
const RETRY_SLEEP_MS: u64 = 10000;

// The following structure is heavily inspired by the underlying starkware-libs/sequencer implementation.
// It uses sequencer's RpcStateReader under the hood in some situations, while in other situation
// the actual implementation has been copied and modified to our needs.

pub struct RpcStateReader {
    chain: RpcChain,
    state: RpcCachedState,
    block_number: BlockNumber,
    inner: GatewayRpcStateReader,
}

impl RpcStateReader {
    pub fn new(chain: RpcChain, block_number: BlockNumber) -> Self {
        let config = build_config(chain);

        Self {
            inner: GatewayRpcStateReader::from_number(&config, block_number),
            chain,
            state: RpcCachedState::default(),
            block_number,
        }
    }

    pub fn load(&mut self) {
        let path = PathBuf::from(format!("rpc_cache/{}.json", self.block_number));
        let Ok(file) = File::open(path) else { return };
        self.state = serde_json::from_reader(file).unwrap();
    }

    pub fn save(&self) {
        let path = PathBuf::from(format!("rpc_cache/{}.json", self.block_number));
        let file = File::create(path).unwrap();
        serde_json::to_writer_pretty(file, &self.state).unwrap();
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

    pub fn get_contract_class(&self, class_hash: &ClassHash) -> StateResult<SNContractClass> {
        let params = json!({
            "block_id": self.inner.block_id,
            "class_hash": class_hash.to_string(),
        });

        serde_json::from_value(self.send_rpc_request_with_retry("starknet_getClass", params)?)
            .map_err(serde_err_to_state_err)
    }

    pub fn get_chain_id(&self) -> ChainId {
        self.chain.into()
    }

    pub fn get_transaction_trace(
        &self,
        hash: &TransactionHash,
    ) -> StateResult<RpcTransactionTrace> {
        let params = json!([hash]);

        serde_json::from_value(
            self.send_rpc_request_with_retry("starknet_traceTransaction", params)?,
        )
        .map_err(serde_err_to_state_err)
    }

    pub fn get_transaction(&self, hash: &TransactionHash) -> StateResult<Transaction> {
        let params = json!([hash]);

        let tx = self.send_rpc_request_with_retry("starknet_getTransactionByHash", params)?;

        objects::deser::transaction_from_json(tx).map_err(serde_err_to_state_err)
    }

    pub fn get_block_info(&self) -> StateResult<BlockInfo> {
        // This function is inspired by sequencer's RpcStateReader::get_block_info

        fn parse_gas_price(price: GasPrice) -> NonzeroGasPrice {
            NonzeroGasPrice::new(price).unwrap_or(NonzeroGasPrice::MIN)
        }

        let params = GetBlockWithTxHashesParams {
            block_id: self.inner.block_id,
        };

        let header: BlockHeader = serde_json::from_value(
            self.send_rpc_request_with_retry("starknet_getBlockWithTxHashes", params)?,
        )
        .map_err(serde_err_to_state_err)?;

        Ok(BlockInfo {
            block_number: header.block_number,
            sequencer_address: header.sequencer_address,
            block_timestamp: header.timestamp,
            gas_prices: validated_gas_prices(
                parse_gas_price(header.l1_gas_price.price_in_wei),
                parse_gas_price(header.l1_gas_price.price_in_fri),
                parse_gas_price(header.l1_data_gas_price.price_in_wei),
                parse_gas_price(header.l1_data_gas_price.price_in_fri),
                NonzeroGasPrice::MIN,
                NonzeroGasPrice::MIN,
            ),
            use_kzg_da: true,
        })
    }

    pub fn get_block_with_tx_hashes(&self) -> StateResult<BlockWithTxHahes> {
        let params = GetBlockWithTxHashesParams {
            block_id: self.inner.block_id,
        };

        serde_json::from_value(
            self.send_rpc_request_with_retry("starknet_getBlockWithTxHashes", params)?,
        )
        .map_err(serde_err_to_state_err)
    }

    pub fn get_transaction_receipt(
        &self,
        hash: &TransactionHash,
    ) -> StateResult<RpcTransactionReceipt> {
        let params = json!([hash]);

        serde_json::from_value(
            self.send_rpc_request_with_retry("starknet_getTransactionReceipt", params)?,
        )
        .map_err(serde_err_to_state_err)
    }

    pub fn get_class_info(&self, class_hash: &ClassHash) -> anyhow::Result<ClassInfo> {
        match self.get_contract_class(class_hash)? {
            SNContractClass::Sierra(sierra) => {
                let abi_length = sierra.abi.len();
                let sierra_length = sierra.sierra_program.len();
                let version = SierraVersion::extract_from_program(&sierra.sierra_program)?;
                Ok(ClassInfo::new(
                    &sierra_to_contact_class_v1(sierra)?,
                    sierra_length,
                    abi_length,
                    version,
                )?)
            }
            SNContractClass::Legacy(legacy) => {
                let abi_length = legacy
                    .abi
                    .clone()
                    .expect("legendary contract should have abi")
                    .len();
                Ok(ClassInfo::new(
                    &legacy_to_contract_class_v0(legacy)?,
                    0,
                    abi_length,
                    SierraVersion::DEPRECATED,
                )?)
            }
        }
    }
}

fn build_config(chain: RpcChain) -> RpcStateReaderConfig {
    let url = match chain {
        RpcChain::MainNet => {
            env::var("RPC_ENDPOINT_MAINNET").expect("Missing env var: RPC_ENDPOINT_MAINNET")
        }
        RpcChain::TestNet => {
            env::var("RPC_ENDPOINT_TESTNET").expect("Missing env var: RPC_ENDPOINT_TESTNET")
        }
        RpcChain::TestNet2 => unimplemented!(),
    };

    RpcStateReaderConfig {
        url,
        json_rpc_version: "2.0".to_string(),
    }
}

impl StateReader for RpcStateReader {
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
        Ok(match self.get_contract_class(&class_hash)? {
            SNContractClass::Legacy(compressed_legacy_cc) => {
                compile_legacy_cc(compressed_legacy_cc)
            }
            SNContractClass::Sierra(flattened_sierra_cc) => {
                compile_sierra_cc(flattened_sierra_cc, class_hash)
            }
        })
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.inner.get_compiled_class_hash(class_hash)
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
        class_hash = class_hash.to_string(),
        length = bytecode_size(&sierra_cc.sierra_program)
    )
    .entered();

    if cfg!(feature = "only_casm") {
        info!("starting vm contract compilation");

        let pre_compilation_instant = Instant::now();

        let casm_cc =
        cairo_lang_starknet_classes::casm_contract_class::CasmContractClass::from_contract_class(sierra_cc, false, usize::MAX).unwrap();

        let compilation_time = pre_compilation_instant.elapsed().as_millis();

        tracing::info!(
            time = compilation_time,
            size = bytecode_size(&casm_cc.bytecode),
            "vm contract compilation finished"
        );

        RunnableCompiledClass::V1(casm_cc.try_into().unwrap())
    } else {
        let executor = if cfg!(feature = "with-sierra-emu") {
            let program = Arc::new(sierra_cc.extract_sierra_program().unwrap());
            sierra_emu::VirtualMachine::new_starknet(program, &sierra_cc.entry_points_by_type)
                .into()
        } else {
            utils::get_native_executor(&sierra_cc, class_hash).into()
        };

        let casm = CasmContractClass::from_contract_class(sierra_cc, false, usize::MAX).unwrap();
        let casm = CompiledClassV1::try_from(casm).unwrap();

        RunnableCompiledClass::V1Native(NativeCompiledClassV1::new(executor, casm))
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

fn bytecode_size(data: &[BigUintAsHex]) -> usize {
    data.iter().map(|n| n.value.to_bytes_be().len()).sum()
}

#[cfg(test)]
mod tests {
    use starknet_api::block::FeeType;

    use super::*;

    #[test]
    fn test_get_block_info() {
        let reader = RpcStateReader::new(RpcChain::MainNet, BlockNumber(169928));

        let block = reader.get_block_info().unwrap();

        assert_eq!(
            block.gas_prices.l1_gas_price(&FeeType::Eth).get().0,
            22804578690
        );
    }

    #[test]
    fn test_get_block_with_tx_hashes() {
        let reader = RpcStateReader::new(RpcChain::MainNet, BlockNumber(397709));

        let block = reader.get_block_with_tx_hashes().unwrap();

        assert_eq!(block.transactions.len(), 211);
    }
}
