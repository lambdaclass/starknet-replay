// Some parts of the `RemoteReader` were inspired from Sequencer's
// `RpcStateReader`. Unlike `RpcStateReader`, this reader only focuses on
// fetching logic. For example, there is no contract compilation.

use std::env;

use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};
use starknet_api::{
    block::BlockNumber,
    core::{ChainId, ClassHash},
};
use starknet_core::types::ContractClass;
use starknet_gateway::rpc_objects::{
    RpcErrorCode, RpcErrorResponse, RpcResponse, RPC_CLASS_HASH_NOT_FOUND,
    RPC_ERROR_BLOCK_NOT_FOUND, RPC_ERROR_CONTRACT_ADDRESS_NOT_FOUND, RPC_ERROR_INVALID_PARAMS,
};
use thiserror::Error;

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
}

pub struct RemoteReader {
    client: Client,
    url: String,
    block_number: BlockNumber,
}

impl RemoteReader {
    pub fn new(url: String, block_number: BlockNumber) -> Self {
        let client = Client::new();

        Self {
            client,
            url,
            block_number,
        }
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
        class_hash: &ClassHash,
    ) -> Result<ContractClass, RemoteReaderError> {
        let params = json!({
            "block_id": {
                "block_number": self.block_number,
            },
            "class_hash": class_hash.to_hex_string(),
        });

        let response = self.send_rpc_request("starknet_getClass", params)?;
        let result = serde_json::from_value(response)?;

        Ok(result)
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
    use starknet_api::{block::BlockNumber, class_hash, core::ChainId};
    use starknet_core::types::ContractClass;

    use super::{url_from_env, RemoteReader};

    #[test]
    pub fn get_contract_class() {
        let url = url_from_env(ChainId::Mainnet);
        let reader = RemoteReader::new(url, BlockNumber(1500000));

        let contract_class = reader
            .get_contract_class(&class_hash!(
                "0x07f3331378862ed0a10f8c3d49f4650eb845af48f1c8120591a43da8f6f12679"
            ))
            .unwrap();

        let ContractClass::Sierra(contract_class) = contract_class else {
            panic!("expected sierra contract class");
        };
        assert_eq!(contract_class.contract_class_version, "0.1.0");
        assert_eq!(contract_class.sierra_program.len(), 7059);
    }
}
