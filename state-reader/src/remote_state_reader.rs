//! This crate contains logic for reading node state from a node, through RPC calls.

// Some parts of the `RemoteReader` were inspired from Sequencer's
// `RpcStateReader`. Unlike `RpcStateReader`, this reader only focuses on
// fetching logic. For example, there is no contract compilation.

use std::{cell::Cell, env, time::Duration};

use apollo_gateway::rpc_objects::{
    RpcResponse, RPC_CLASS_HASH_NOT_FOUND, RPC_ERROR_BLOCK_NOT_FOUND,
    RPC_ERROR_CONTRACT_ADDRESS_NOT_FOUND, RPC_ERROR_INVALID_PARAMS,
};
use blockifier_reexecution::state_reader::serde_utils::deserialize_transaction_json_to_starknet_api_tx;
use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};
use starknet_api::{
    block::BlockNumber,
    core::{ChainId, ClassHash, ContractAddress, Nonce},
    state::StorageKey,
    transaction::{Transaction, TransactionHash},
};
use starknet_core::types::{BlockWithTxHashes, ContractClass, Felt};

use crate::{error::StateReaderError, objects::RpcTransactionReceipt};

/// Reads a Starknet node's state through RPC.
pub struct RemoteStateReader {
    client: Client,
    url: String,
    timeout_counter: Cell<u64>,
}

const DEFAULT_RPC_RETRY_LIMIT: u32 = 10;
const DEFAULT_RPC_TIMEOUT_SECS: u64 = 90;

impl RemoteStateReader {
    pub fn new(url: String) -> Self {
        let client = {
            let timeout = std::env::var("RPC_TIMEOUT")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(DEFAULT_RPC_TIMEOUT_SECS);

            let timeout_secs = Duration::from_secs(timeout);

            Client::builder().timeout(timeout_secs).build().unwrap()
        };

        Self {
            client,
            url,
            timeout_counter: Cell::new(0),
        }
    }

    pub fn get_timeout_counter(&self) -> u64 {
        self.timeout_counter.get()
    }

    pub fn reset_counters(&self) {
        self.timeout_counter.set(0);
    }

    /// Sends a RPC request and retries if a timeout is returned. By default, the limit of retries is set
    /// to 10  before failing. The `RPC_RETRY_LIMIT` env var is to change the number of retries.
    fn send_rpc_request_with_retry(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, StateReaderError> {
        let retry_limit = std::env::var("RPC_RETRY_LIMIT")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(DEFAULT_RPC_RETRY_LIMIT);

        for retry_instance in 0..retry_limit {
            match self.send_rpc_request(method, &params) {
                Ok(response) => return Ok(response),
                Err(StateReaderError::BadHttpStatusCode(
                    StatusCode::GATEWAY_TIMEOUT | StatusCode::REQUEST_TIMEOUT,
                )) => {
                    tracing::warn!(
                        "Retrying request, remaining tries: {}",
                        retry_limit - retry_instance
                    );
                    self.timeout_counter.set(self.timeout_counter.get() + 1);

                    let backoff_timeout = {
                        let backoff_timeout = rand::random_range(0..2u64.pow(retry_instance));
                        Duration::from_secs(backoff_timeout)
                    };
                    std::thread::sleep(backoff_timeout);

                    continue;
                }
                // if there's actually an error, different from a timeout, it should be returned.
                err => return err,
            }
        }
        Err(StateReaderError::RpcRequestTimeout)
    }

    fn send_rpc_request(&self, method: &str, params: &Value) -> Result<Value, StateReaderError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": method,
            "params": params,
        });

        let response = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()?;

        if !response.status().is_success() {
            return Err(StateReaderError::BadHttpStatusCode(response.status()));
        }

        let response: RpcResponse = response.json()?;

        match response {
            RpcResponse::Success(rpc_success_response) => Ok(rpc_success_response.result),
            RpcResponse::Error(rpc_error_response) => match rpc_error_response.error.code {
                RPC_ERROR_BLOCK_NOT_FOUND => Err(StateReaderError::BlockNotFound),
                RPC_ERROR_CONTRACT_ADDRESS_NOT_FOUND => {
                    Err(StateReaderError::ContractAddressNotFound)
                }
                RPC_CLASS_HASH_NOT_FOUND => Err(StateReaderError::ClassHashNotFound),
                RPC_ERROR_INVALID_PARAMS => {
                    Err(StateReaderError::InvalidRpcParams(rpc_error_response))
                }
                _ => Err(StateReaderError::UnexpectedRpcErrorCode(
                    rpc_error_response.error.code,
                )),
            },
        }
    }

    pub fn get_contract_class(
        &self,
        block_number: BlockNumber,
        class_hash: &ClassHash,
    ) -> Result<ContractClass, StateReaderError> {
        let params = json!({
            "block_id": {
                "block_number": block_number,
            },
            "class_hash": class_hash.to_hex_string(),
        });

        let response = self.send_rpc_request_with_retry("starknet_getClass", params)?;
        let result = serde_json::from_value(response)?;

        Ok(result)
    }

    pub fn get_block_with_tx_hashes(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockWithTxHashes, StateReaderError> {
        let params = json!({
            "block_id": {
                "block_number": block_number,
            },
        });

        let response = self.send_rpc_request_with_retry("starknet_getBlockWithTxHashes", params)?;
        let result = serde_json::from_value(response)?;
        Ok(result)
    }

    pub fn get_tx(&self, hash: &TransactionHash) -> Result<Transaction, StateReaderError> {
        let params = json!([hash]);

        let response = self.send_rpc_request_with_retry("starknet_getTransactionByHash", params)?;
        let tx = deserialize_transaction_json_to_starknet_api_tx(response)?;

        Ok(tx)
    }

    pub fn get_tx_receipt(
        &self,
        hash: &TransactionHash,
    ) -> Result<RpcTransactionReceipt, StateReaderError> {
        let params = json!([hash]);

        let response =
            self.send_rpc_request_with_retry("starknet_getTransactionReceipt", params)?;
        let result = serde_json::from_value(response)?;
        Ok(result)
    }

    pub fn get_storage_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> Result<Felt, StateReaderError> {
        let params = json!({
            "block_id": {
                "block_number": block_number,
            },
            "contract_address": contract_address,
            "key": key,
        });

        let response = self.send_rpc_request_with_retry("starknet_getStorageAt", params);

        match response {
            Ok(response) => Ok(serde_json::from_value(response)?),
            Err(StateReaderError::ContractAddressNotFound) => Ok(Felt::default()),
            Err(err) => Err(err)?,
        }
    }

    pub fn get_nonce_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> Result<Nonce, StateReaderError> {
        let params = json!({
            "block_id": {
                "block_number": block_number,
            },
            "contract_address": contract_address,
        });

        let response = self.send_rpc_request_with_retry("starknet_getNonce", params);

        match response {
            Ok(response) => Ok(serde_json::from_value(response)?),
            Err(StateReaderError::ContractAddressNotFound) => Ok(Nonce::default()),
            Err(err) => Err(err)?,
        }
    }

    pub fn get_class_hash_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> Result<ClassHash, StateReaderError> {
        let params = json!({
            "block_id": {
                "block_number": block_number,
            },
            "contract_address": contract_address,
        });

        let response = self.send_rpc_request_with_retry("starknet_getClassHashAt", params);

        match response {
            Ok(response) => Ok(serde_json::from_value(response)?),
            Err(StateReaderError::ContractAddressNotFound) => Ok(ClassHash::default()),
            Err(err) => Err(err)?,
        }
    }

    pub fn get_chain_id(&self) -> Result<ChainId, StateReaderError> {
        let params = json!([]);

        let response = self.send_rpc_request_with_retry("starknet_chainId", params)?;

        let chain_id_hex: String = serde_json::from_value(response)?;
        let chain_id_hex = chain_id_hex.strip_prefix("0x").unwrap_or(&chain_id_hex);

        let chain_id_bytes = hex::decode(chain_id_hex)?;
        let chain_id_string = String::from_utf8(chain_id_bytes)?;

        Ok(ChainId::from(chain_id_string))
    }
}

pub fn url_from_env(chain: &ChainId) -> String {
    match chain {
        ChainId::Mainnet => {
            env::var("RPC_ENDPOINT_MAINNET").expect("Missing env var: RPC_ENDPOINT_MAINNET")
        }
        ChainId::Sepolia => {
            env::var("RPC_ENDPOINT_TESTNET").expect("Missing env var: RPC_ENDPOINT_TESTNET")
        }
        _ => unimplemented!("unsupported chain"),
    }
}

#[cfg(test)]
mod tests {
    use starknet_api::{
        block::BlockNumber,
        class_hash, contract_address,
        core::{ChainId, Nonce},
        felt, storage_key,
        transaction::{fields::Fee, InvokeTransaction, Transaction, TransactionHash},
    };
    use starknet_core::types::{BlockStatus, ContractClass};

    use super::{url_from_env, RemoteStateReader};

    #[test]
    pub fn get_contract_class() {
        let url = url_from_env(&ChainId::Mainnet);
        let reader = RemoteStateReader::new(url);

        let contract_class = reader
            .get_contract_class(
                BlockNumber(1500000),
                &class_hash!("0x07f3331378862ed0a10f8c3d49f4650eb845af48f1c8120591a43da8f6f12679"),
            )
            .unwrap();

        let ContractClass::Sierra(contract_class) = contract_class else {
            panic!("expected sierra contract class");
        };
        assert_eq!(contract_class.contract_class_version, "0.1.0");
        assert_eq!(contract_class.sierra_program.len(), 7059);
    }

    #[test]
    pub fn get_block_with_tx_hashes() {
        let url = url_from_env(&ChainId::Mainnet);
        let reader = RemoteStateReader::new(url);

        let block = reader
            .get_block_with_tx_hashes(BlockNumber(1500000))
            .unwrap();

        assert_eq!(block.status, BlockStatus::AcceptedOnL1);
        assert_eq!(block.transactions.len(), 22);
    }

    #[test]
    pub fn get_tx() {
        let url = url_from_env(&ChainId::Mainnet);
        let reader = RemoteStateReader::new(url);

        let tx = reader
            .get_tx(&TransactionHash(felt!(
                "0x04762bb00f9c71c748744d1d797ccd15396c22383a9fb40726e779a3322bbb64"
            )))
            .unwrap();

        let Transaction::Invoke(InvokeTransaction::V1(tx)) = tx else {
            panic!("expected invoke 0x1 transaction")
        };
        assert_eq!(tx.max_fee, Fee(923473295801928));
        assert_eq!(tx.nonce, Nonce(felt!("0xa")));
    }

    #[test]
    pub fn get_tx_receipt() {
        let url = url_from_env(&ChainId::Mainnet);
        let reader = RemoteStateReader::new(url);

        let tx_receipt = reader
            .get_tx_receipt(&TransactionHash(felt!(
                "0x04762bb00f9c71c748744d1d797ccd15396c22383a9fb40726e779a3322bbb64"
            )))
            .unwrap();

        assert_eq!(tx_receipt.messages_sent.len(), 0);
        assert_eq!(tx_receipt.events.len(), 6);
    }

    #[test]
    pub fn get_storage_at() {
        let url = url_from_env(&ChainId::Mainnet);
        let reader = RemoteStateReader::new(url);

        let value = reader
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
    pub fn get_nonce_at() {
        let url = url_from_env(&ChainId::Mainnet);
        let reader = RemoteStateReader::new(url);

        let value = reader
            .get_nonce_at(
                BlockNumber(1500000),
                contract_address!(
                    "0x055e557a4c975059522a1321d7a7bd215287450907419e5f8aa98145c7699a2c"
                ),
            )
            .unwrap();

        assert_eq!(value, Nonce(felt!("0x7d080")));
    }

    #[test]
    pub fn get_class_hash_at() {
        let url = url_from_env(&ChainId::Mainnet);
        let reader = RemoteStateReader::new(url);

        let value = reader
            .get_class_hash_at(
                BlockNumber(1500000),
                contract_address!(
                    "0x055e557a4c975059522a1321d7a7bd215287450907419e5f8aa98145c7699a2c"
                ),
            )
            .unwrap();

        assert_eq!(
            value,
            class_hash!("0x1a736d6ed154502257f02b1ccdf4d9d1089f80811cd6acad48e6b6a9d1f2003")
        );
    }

    #[test]
    pub fn get_chain_id() {
        let url = url_from_env(&ChainId::Mainnet);
        let reader = RemoteStateReader::new(url);
        let value = reader.get_chain_id().unwrap();
        assert_eq!(value, ChainId::Mainnet);

        let url = url_from_env(&ChainId::Sepolia);
        let reader = RemoteStateReader::new(url);
        let value = reader.get_chain_id().unwrap();
        assert_eq!(value, ChainId::Sepolia);
    }
}
