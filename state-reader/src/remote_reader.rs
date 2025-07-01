// Some parts of the `RemoteReader` were inspired from Sequencer's
// `RpcStateReader`. Unlike `RpcStateReader`, this reader only focuses on
// fetching logic. For example, there is no contract compilation.

use std::{env, string::FromUtf8Error};

use blockifier::state::errors::StateError;
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
use starknet_gateway::rpc_objects::{
    RpcErrorCode, RpcErrorResponse, RpcResponse, RPC_CLASS_HASH_NOT_FOUND,
    RPC_ERROR_BLOCK_NOT_FOUND, RPC_ERROR_CONTRACT_ADDRESS_NOT_FOUND, RPC_ERROR_INVALID_PARAMS,
};
use thiserror::Error;

use crate::objects::RpcTransactionReceipt;

#[derive(Debug, Error)]
pub enum RemoteReaderError {
    #[error("block not found")]
    BlockNotFound,
    #[error("class hash not found")]
    ClassHashNotFound,
    #[error("contract address not found")]
    ContractAddressNotFound,
    #[error("invalid params: {0:?}")]
    InvalidParams(RpcErrorResponse),
    #[error("bad status: {0}")]
    BadStatus(StatusCode),
    #[error("unexpected error code: {0}")]
    UnexpectedErrorCode(RpcErrorCode),
    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
    #[error(transparent)]
    ParseError(#[from] serde_json::Error),
    #[error(transparent)]
    FromHexError(#[from] hex::FromHexError),
    #[error(transparent)]
    FromUtf8Error(#[from] FromUtf8Error),
}

pub struct RemoteReader {
    client: Client,
    url: String,
}

impl RemoteReader {
    pub fn new(url: String) -> Self {
        let client = Client::new();

        Self { client, url }
    }

    pub fn send_rpc_request(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, RemoteReaderError> {
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
            return Err(RemoteReaderError::BadStatus(response.status()));
        };

        let response: RpcResponse = response.json()?;

        match response {
            RpcResponse::Success(rpc_success_response) => Ok(rpc_success_response.result),
            RpcResponse::Error(rpc_error_response) => match rpc_error_response.error.code {
                RPC_ERROR_BLOCK_NOT_FOUND => Err(RemoteReaderError::BlockNotFound),
                RPC_ERROR_CONTRACT_ADDRESS_NOT_FOUND => {
                    Err(RemoteReaderError::ContractAddressNotFound)
                }
                RPC_CLASS_HASH_NOT_FOUND => Err(RemoteReaderError::ClassHashNotFound),
                RPC_ERROR_INVALID_PARAMS => {
                    Err(RemoteReaderError::InvalidParams(rpc_error_response))
                }
                _ => Err(RemoteReaderError::UnexpectedErrorCode(
                    rpc_error_response.error.code,
                )),
            },
        }
    }

    pub fn get_contract_class(
        &self,
        block_number: BlockNumber,
        class_hash: &ClassHash,
    ) -> Result<ContractClass, RemoteReaderError> {
        let params = json!({
            "block_id": {
                "block_number": block_number,
            },
            "class_hash": class_hash.to_hex_string(),
        });

        let response = self.send_rpc_request("starknet_getClass", params)?;
        let result = serde_json::from_value(response)?;

        Ok(result)
    }

    pub fn get_block_with_tx_hashes(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockWithTxHashes, RemoteReaderError> {
        let params = json!({
            "block_id": {
                "block_number": block_number,
            },
        });

        let response = self.send_rpc_request("starknet_getBlockWithTxHashes", params)?;
        let result = serde_json::from_value(response)?;
        Ok(result)
    }

    pub fn get_tx(&self, hash: &TransactionHash) -> Result<Transaction, RemoteReaderError> {
        let params = json!([hash]);

        let response = self.send_rpc_request("starknet_getTransactionByHash", params)?;
        let tx = deserialize_transaction_json_to_starknet_api_tx(response)?;

        Ok(tx)
    }

    pub fn get_tx_receipt(
        &self,
        hash: &TransactionHash,
    ) -> Result<RpcTransactionReceipt, RemoteReaderError> {
        let params = json!([hash]);

        let response = self.send_rpc_request("starknet_getTransactionReceipt", params)?;
        let result = serde_json::from_value(response)?;
        Ok(result)
    }

    pub fn get_storage_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> Result<Felt, RemoteReaderError> {
        let params = json!({
            "block_id": {
                "block_number": block_number,
            },
            "contract_address": contract_address,
            "key": key,
        });

        let response = self.send_rpc_request("starknet_getStorageAt", params);

        match response {
            Ok(response) => Ok(serde_json::from_value(response)?),
            Err(RemoteReaderError::ContractAddressNotFound) => Ok(Felt::default()),
            Err(err) => Err(err)?,
        }
    }

    pub fn get_nonce_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> Result<Nonce, RemoteReaderError> {
        let params = json!({
            "block_id": {
                "block_number": block_number,
            },
            "contract_address": contract_address,
        });

        let response = self.send_rpc_request("starknet_getNonce", params);

        match response {
            Ok(response) => Ok(serde_json::from_value(response)?),
            Err(RemoteReaderError::ContractAddressNotFound) => Ok(Nonce::default()),
            Err(err) => Err(err)?,
        }
    }

    pub fn get_class_hash_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> Result<ClassHash, RemoteReaderError> {
        let params = json!({
            "block_id": {
                "block_number": block_number,
            },
            "contract_address": contract_address,
        });

        let response = self.send_rpc_request("starknet_getClassHashAt", params);

        match response {
            Ok(response) => Ok(serde_json::from_value(response)?),
            Err(RemoteReaderError::ContractAddressNotFound) => Ok(ClassHash::default()),
            Err(err) => Err(err)?,
        }
    }

    pub fn get_chain_id(&self) -> Result<ChainId, RemoteReaderError> {
        let params = json!([]);

        let response = self.send_rpc_request("starknet_chainId", params)?;

        let chain_id_hex: String = serde_json::from_value(response)?;
        let chain_id_hex = chain_id_hex.strip_prefix("0x").unwrap_or(&chain_id_hex);

        let chain_id_bytes = hex::decode(chain_id_hex)?;
        let chain_id_string = String::from_utf8(chain_id_bytes)?;

        Ok(ChainId::from(chain_id_string))
    }
}

impl Into<StateError> for RemoteReaderError {
    fn into(self) -> StateError {
        StateError::StateReadError(self.to_string())
    }
}

pub fn url_from_env(chain: ChainId) -> String {
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

    use super::{url_from_env, RemoteReader};

    #[test]
    pub fn get_contract_class() {
        let url = url_from_env(ChainId::Mainnet);
        let reader = RemoteReader::new(url);

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
        let url = url_from_env(ChainId::Mainnet);
        let reader = RemoteReader::new(url);

        let block = reader
            .get_block_with_tx_hashes(BlockNumber(1500000))
            .unwrap();

        assert_eq!(block.status, BlockStatus::AcceptedOnL1);
        assert_eq!(block.transactions.len(), 22);
    }

    #[test]
    pub fn get_tx() {
        let url = url_from_env(ChainId::Mainnet);
        let reader = RemoteReader::new(url);

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
        let url = url_from_env(ChainId::Mainnet);
        let reader = RemoteReader::new(url);

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
        let url = url_from_env(ChainId::Mainnet);
        let reader = RemoteReader::new(url);

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
        let url = url_from_env(ChainId::Mainnet);
        let reader = RemoteReader::new(url);

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
        let url = url_from_env(ChainId::Mainnet);
        let reader = RemoteReader::new(url);

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
        let url = url_from_env(ChainId::Mainnet);
        let reader = RemoteReader::new(url);
        let value = reader.get_chain_id().unwrap();
        assert_eq!(value, ChainId::Mainnet);

        let url = url_from_env(ChainId::Sepolia);
        let reader = RemoteReader::new(url);
        let value = reader.get_chain_id().unwrap();
        assert_eq!(value, ChainId::Sepolia);
    }
}
