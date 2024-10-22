use serde::{Deserialize, Serialize};
use starknet::core::types::Transaction;
use starknet_api::{block::BlockStatus, transaction::TransactionHash};
use starknet_gateway::rpc_objects::BlockHeader;

// The following structures are taken from https://github.com/starkware-libs/sequencer,
// but modified to suit our particular needs.

#[derive(Debug, Deserialize, Serialize)]
pub struct BlockWithTxHahes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<BlockStatus>,
    #[serde(flatten)]
    pub header: BlockHeader,
    pub transactions: Vec<TransactionHash>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BlockWithTxs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<BlockStatus>,
    #[serde(flatten)]
    pub header: BlockHeader,
    pub transactions: Vec<TransactionWithHash>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransactionWithHash {
    pub transaction_hash: TransactionHash,
    #[serde(flatten)]
    pub transaction: Transaction,
}
