use std::{io, string::FromUtf8Error};

use apollo_gateway::rpc_objects::{RpcErrorCode, RpcErrorResponse};
use cairo_lang_starknet_classes::casm_contract_class::StarknetSierraCompilationError;
use cairo_vm::types::errors::program_errors::ProgramError;
use reqwest::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StateReaderError {
    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
    #[error(transparent)]
    FromHexError(#[from] hex::FromHexError),
    #[error(transparent)]
    FromUtf8Error(#[from] FromUtf8Error),
    #[error(transparent)]
    CairoNativeError(#[from] cairo_native::error::Error),
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error(transparent)]
    LockfileError(#[from] lockfile::Error),
    #[error(transparent)]
    ProgramError(#[from] ProgramError),
    #[error(transparent)]
    StarknetApiError(#[from] starknet_api::StarknetApiError),
    #[error(transparent)]
    StarknetSierraCompilationError(#[from] StarknetSierraCompilationError),
    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::Error),
    #[error("block not found")]
    BlockNotFound,
    #[error("class hash not found")]
    ClassHashNotFound,
    #[error("contract address not found")]
    ContractAddressNotFound,
    #[error("invalid params: {0:?}")]
    InvalidRpcParams(Box<RpcErrorResponse>),
    #[error("bad status: {0}")]
    BadHttpStatusCode(StatusCode),
    #[error("unexpected error code: {0}")]
    UnexpectedRpcErrorCode(RpcErrorCode),
    #[error("a legacy contract should always have an ABI")]
    LegacyContractWithoutAbi,
    #[error("the received sierra class was invalid")]
    InvalidSierraClass,
    #[error("reached the limit of retries for the RPC request")]
    RpcRequestTimeout,
}
