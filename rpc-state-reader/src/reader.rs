use std::{env, fmt, num::NonZeroU128, sync::Arc};

use blockifier::{
    blockifier::block::{BlockInfo, GasPrices},
    execution::contract_class::{
        ContractClass, ContractClassV0, ContractClassV0Inner, NativeContractClassV1,
    },
    state::state_api::{StateReader, StateResult},
};
use cairo_vm::types::program::Program;
use serde::Serialize;
use serde_json::Value;
use starknet::core::types::ContractClass as SNContractClass;
use starknet_api::{
    block::{BlockNumber, GasPrice},
    core::{ChainId, ClassHash, CompiledClassHash, ContractAddress},
    state::StorageKey,
    transaction::{Transaction, TransactionHash},
};
use starknet_gateway::{
    config::RpcStateReaderConfig,
    errors::{serde_err_to_state_err, RPCStateReaderError, RPCStateReaderResult},
    rpc_objects::{BlockHeader, GetBlockWithTxHashesParams, GetStorageAtParams},
    rpc_state_reader::RpcStateReader as GatewayRpcStateReader,
};
use ureq::json;

use crate::{
    objects::{self, BlockWithTxHahes, BlockWithTxs, RpcTransactionReceipt, RpcTransactionTrace},
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

const MAX_RETRIES: u32 = 8;

// The following structured is heavily inspired by the underlying starkware-libs/sequencer implementation.
// It uses sequencer's RpcStateReader under the hood in some situations, while in other situation
// the actual implementation has been copied and modified to our needs.

pub struct RpcStateReader {
    chain: RpcChain,
    inner: GatewayRpcStateReader,
}

impl RpcStateReader {
    pub fn new(chain: RpcChain, block_number: BlockNumber) -> Self {
        let config = build_config(chain);

        Self {
            inner: GatewayRpcStateReader::from_number(&config, block_number),
            chain,
        }
    }

    pub fn new_latest(chain: RpcChain) -> Self {
        let config = build_config(chain);

        Self {
            inner: GatewayRpcStateReader::from_latest(&config),
            chain,
        }
    }

    pub fn send_rpc_request_with_retry(
        &self,
        method: &str,
        params: impl Serialize,
    ) -> RPCStateReaderResult<Value> {
        retry(|| self.inner.send_rpc_request(method, &params))
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

        fn parse_gas_price(price: GasPrice) -> NonZeroU128 {
            NonZeroU128::new(price.0).unwrap_or(NonZeroU128::MIN)
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
            gas_prices: GasPrices::new(
                parse_gas_price(header.l1_gas_price.price_in_wei),
                parse_gas_price(header.l1_gas_price.price_in_fri),
                parse_gas_price(header.l1_data_gas_price.price_in_wei),
                parse_gas_price(header.l1_data_gas_price.price_in_fri),
                NonZeroU128::MIN,
                NonZeroU128::MIN,
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

    pub fn get_block_with_txs(&self) -> StateResult<BlockWithTxs> {
        let params = GetBlockWithTxHashesParams {
            block_id: self.inner.block_id,
        };

        serde_json::from_value(
            self.send_rpc_request_with_retry("starknet_getBlockWithTxs", params)?,
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

        retry(|| {
            let result = self
                .inner
                .send_rpc_request("starknet_getStorageAt", &get_storage_at_params);
            match result {
                Ok(value) => Ok(serde_json::from_value(value).map_err(serde_err_to_state_err)?),
                Err(RPCStateReaderError::ContractAddressNotFound(_)) => {
                    Ok(cairo_vm::Felt252::default())
                }
                Err(e) => Err(e)?,
            }
        })
    }

    fn get_nonce_at(
        &self,
        contract_address: ContractAddress,
    ) -> StateResult<starknet_api::core::Nonce> {
        retry(|| self.inner.get_nonce_at(contract_address))
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        retry(|| self.inner.get_class_hash_at(contract_address))
    }

    fn get_compiled_contract_class(&self, class_hash: ClassHash) -> StateResult<ContractClass> {
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
) -> ContractClass {
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

        ContractClass::V1Native(NativeContractClassV1::new(executor, sierra_cc).unwrap())
    }
}

fn compile_legacy_cc(
    compressed_legacy_cc: starknet::core::types::CompressedLegacyContractClass,
) -> ContractClass {
    let as_str = utils::decode_reader(compressed_legacy_cc.program).unwrap();
    let program = Program::from_bytes(as_str.as_bytes(), None).unwrap();
    let entry_points_by_type =
        utils::map_entry_points_by_type_legacy(compressed_legacy_cc.entry_points_by_type);
    let inner = Arc::new(ContractClassV0Inner {
        program,
        entry_points_by_type,
    });
    ContractClass::V0(ContractClassV0(inner))
}

/// Retries the closure `MAX_RETRIES` times, until Ok is returned
fn retry<A, B>(f: impl Fn() -> Result<A, B>) -> Result<A, B> {
    let result = f();
    if result.is_ok() {
        return result;
    }

    let mut attempt = 1;
    while attempt < MAX_RETRIES {
        let result = f();
        if result.is_ok() {
            return result;
        }
        attempt += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU128;

    use super::*;

    #[test]
    fn test_get_block_info() {
        let reader = RpcStateReader::new(RpcChain::MainNet, BlockNumber(169928));

        let block = reader.get_block_info().unwrap();

        assert_eq!(
            block
                .gas_prices
                .get_l1_gas_price_by_fee_type(&blockifier::transaction::objects::FeeType::Eth),
            NonZeroU128::new(22804578690).unwrap()
        );
    }

    #[test]
    fn test_get_block_with_tx_hashes() {
        let reader = RpcStateReader::new(RpcChain::MainNet, BlockNumber(397709));

        let block = reader.get_block_with_tx_hashes().unwrap();

        assert_eq!(block.transactions.len(), 211);
    }
}
